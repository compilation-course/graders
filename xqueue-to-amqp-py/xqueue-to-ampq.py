#!/usr/bin/env python3

import asyncio
import aiohttp
import aiormq
import json
import logging
import os
import yaml
import sys


class XQueueException(Exception):
    pass


async def xqueue_post(http_client, url, data):
    async with http_client.post(url, data=data) as resp:
        if resp.status != 200:
            raise XQueueException(
                'error status {} for POST {} with data {}'.format(
                    resp.status, url, data))
        try:
            return await resp.json(content_type='text/html')
        except:
            raise XQueueException(
                'error decoding body for POST {} with data {}'.format(
                    url, data))


async def xqueue_get(http_client, url, params):
    async with http_client.get(url, params=params) as resp:
        if resp.status != 200:
            raise XQueueException(
                'error status {} for GET {} with data {}'.format(
                    resp.status, url, data))
        try:
            return await resp.json(content_type='text/html')
        except:
            raise XQueueException(
                'error decoding body for GET {} with data {}'.format(
                    url, data))


async def xqueue_login(http_client, config):
    """
    login into EDX xqueue, session cookies are set inside the http_client
    cookie_jar
    """
    url = config['xqueue']['base_url'] + '/xqueue/login/'
    data = dict(username=config['xqueue']['username'],
                password=config['xqueue']['password'])
    body = await xqueue_post(http_client, url, data)
    if body['content'] != 'Logged in':
        raise XQueueException('xqueue login failed')


async def xqueue_getqueuelen(http_client, config):
    """
    returns the number of unprocessed submissions in the xqueue
    """
    url = config['xqueue']['base_url'] + "/xqueue/get_queuelen/"
    params = dict(queue_name=config['xqueue']['username'])
    body = await xqueue_get(http_client, url, params)
    return int(body['content'])


async def xqueue_getsubmission(http_client, config):
    """
    consumes and returns one submission from the xqueue.
    the returned submission follow the forma
        {
          xqueue_files: {
            "submitted_file.zip": "http://<url to download zip>"
          },
          xqueue_header: <opaque json object that must be passed when pushing
          the grade>,
          xqueue_body: {
            "student_info": <anonymized student information>,
            "grader_payload": {"lab": "<name of the lab we must grade>"}
          }
        }
    """
    url = config['xqueue']['base_url'] + '/xqueue/get_submission/'
    params = dict(queue_name=config['xqueue']['username'])
    body = await xqueue_get(http_client, url, params)
    logging.info('Received xqueue submission: {}'.format(body['content']))
    content = json.loads(body['content'])
    # XQueue response is quite strange; inner json objects are passed as strings
    # they must be recursively parsed...
    xqueue_body = json.loads(content['xqueue_body'])
    xqueue_body['student_info'] = json.loads(xqueue_body['student_info'])
    xqueue_body['grader_payload'] = json.loads(xqueue_body['grader_payload'])
    return dict(xqueue_files=json.loads(content['xqueue_files']),
                xqueue_header=json.loads(content['xqueue_header']),
                xqueue_body=xqueue_body)


async def xqueue_putresult(http_client, config, xqueue_header, grade):
    """
    publishes a grade on EDX xqueue
        xqueue_header is the opaque object retrieved with the submission
        grade is a dictionnary with the following format
          { "correct": False, "score": 7, "msg" : "html message for the student" }
    """
    url = config['xqueue'][
        'base_url'] + "/xqueue/put_result/?queue_name=" + config['xqueue'][
            'username']
    data = dict(xqueue_header=xqueue_header, xqueue_body=json.dumps(grade))
    body = await xqueue_post(http_client, url, data)
    if body['return_code'] != 0:
        raise XQueueException('xqueue putresult ({}) failed: {}'.format(
            xqueue_header, body['content']))


def signal_to_explanation(signal):
    """
    converts a grader signal to a human readable explanation
    """
    explanations = {
        4: 'illegal instruction',
        6: 'abort, possibly because of a failed assertion',
        8: 'arithmetic exception',
        9:
        'program killed, possibly because of an infinite loop or memory exhaustion',
        10: 'bus error',
        11: 'segmentation fault'
    }
    if signal in explanations:
        return explanations[signal]
    else:
        return "crash"


def parse_yaml_result(yaml_result):
    """
    converts the grader yaml output to a grading payload
      { "correct": False, "score": 7, "msg" : "html message for the student" }
    """
    result = yaml.load(yaml_result, Loader=yaml.SafeLoader)
    correct = (result['grade'] == result['max-grade'])
    score = result['grade']

    if correct:
        msg = "<h1>Congratulations, all tests passed!</h1>"
    else:
        if 'explanation' in result:
            msg = "<h1>There has been an error with your submission: </h1>"
            msg += result['explanation']
        else:
            msg = "<h1>Some tests are failing in your submission: </h1>"
            for group in result['groups']:
                msg += "<h2>{} (grade {} / {})</h2>".format(
                    group['description'], group['grade'], group['max-grade'])
                msg += "<ul>"
                for test in group['tests']:
                    if not test['success']:
                        if 'signal' in test:
                            explanation = ': ' + signal_to_explanation(
                                test['signal'])
                        else:
                            explanation = ''
                        msg += "<li> <b>{}</b> (coefficient {}) failed {} </li>".format(
                            test['description'], test['coefficient'],
                            explanation)
                msg += "</ul>"
            msg += "<br/>"

    return dict(correct=correct, score=score, msg=msg)


async def grade_submission(config, amqp_channel, submission):
    """ sends a submission to amqp-to-test so it is graded """
    zip_name = config['xqueue']['zip_name']
    payload = dict(job_name="xqueue:{}".format(
        submission['xqueue_header']['submission_id']),
                   lab=submission['xqueue_body']['grader_payload']['lab'],
                   dir="dragon-tiger",
                   zip_url=submission['xqueue_files'][zip_name],
                   result_queue=config['xqueue']['amqp_queue'],
                   opaque=json.dumps(submission['xqueue_header']))

    logging.info("sent amqp message to grader:{}\n".format(payload))
    await amqp_channel.basic_publish(json.dumps(payload).encode(),
                                     exchange='grader',
                                     routing_key='lab')


async def on_grader_message(message: aiormq.types.DeliveredMessage, config):
    """ called when we receive the result of a submission graded by amqp-to-test """
    logging.info("received amqp message:{}\n{}\n".format(
        message.delivery.routing_key, message.body))

    body = json.loads(message.body)
    grade = parse_yaml_result(body['yaml_result'])
    xqueue_header = body['opaque']

    async with aiohttp.ClientSession() as session:
        await xqueue_login(session, config)
        try:
            await xqueue_putresult(session, config, xqueue_header, grade)
        except XQueueException as e:
            logging.warning("Failed to send back result to EDX: {}".format(e))
            logging.warning("Maybe the submission_id and key expired")
            # If the submission is not sent back in a timely manner, EDX
            # submission key expires. In that case, the student should resubmit
            # and we better ack to clear the expired job from amqp queue

    await message.channel.basic_ack(message.delivery.delivery_tag)


async def poll_xqueue(config, amqp_channel):
    """ poll xqueue and process waiting submissions """
    async with aiohttp.ClientSession() as session:
        await xqueue_login(session, config)
        queue_len = await xqueue_getqueuelen(session, config)
        logging.info("Polling xqueue ({} jobs submitted)".format(queue_len))
        if queue_len > 0:
            submission = await xqueue_getsubmission(session, config)
            await grade_submission(config, amqp_channel, submission)
            queue_len -= 1
        return queue_len


async def main(config):
    """ main event loop """
    amqp_uri = "amqp://{}:{}/".format(config['amqp']['host'],
                                      config['amqp']['port'])
    logging.info("Connecting to amqp server " + amqp_uri)
    amqp_connection = await aiormq.connect(amqp_uri)
    amqp_channel = await amqp_connection.channel()
    logging.info("Connected to amqp server")

    grader_ok = await amqp_channel.queue_declare(
        config['xqueue']['amqp_queue'], durable='false')

    async def callback(message: aiormq.types.DeliveredMessage):
        await on_grader_message(message, config)

    await amqp_channel.basic_consume(grader_ok.queue, callback)

    await amqp_channel.exchange_declare(exchange=config['amqp']['exchange'],
                                        exchange_type='direct',
                                        durable='false')

    while (1):
        queue_len = await poll_xqueue(config, amqp_channel)
        # throttle requests as per EDX documentation when queue is not growing
        if queue_len == 0:
            await asyncio.sleep(config['xqueue']['poll_delay'])


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("usage: {} config.yml".format(sys.argv[0]))
        sys.exit(1)
    else:
        with open(sys.argv[1], 'r') as cf:
            config = yaml.load(cf.read(), Loader=yaml.SafeLoader)

    log_level = os.environ.get('LOGLEVEL', 'INFO').upper()
    logging.basicConfig(level=log_level)
    logging.info('started logger at {} level'.format(log_level))
    asyncio.run(main(config))
