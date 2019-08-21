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
    url = config['xqueue']['base_url'] + '/xqueue/login/'
    data = dict(username=config['xqueue']['username'],
                password=config['xqueue']['password'])
    body = await xqueue_post(http_client, url, data)
    if body['content'] != 'Logged in':
        raise XQueueException('xqueue login failed')


async def xqueue_getqueuelen(http_client, config):
    url = config['xqueue']['base_url'] + "/xqueue/get_queuelen/"
    params = dict(queue_name=config['xqueue']['username'])
    body = await xqueue_get(http_client, url, params)
    return int(body['content'])


async def xqueue_getsubmission(http_client, config):
    url = config['xqueue']['base_url'] + '/xqueue/get_submission/'
    params = dict(queue_name=config['xqueue']['username'])
    body = await xqueue_get(http_client, url, params)
    logging.info('Received xqueue submission: {}'.format(body['content']))
    content = json.loads(body['content'])
    xqueue_body = json.loads(content['xqueue_body'])
    xqueue_body['student_info'] = json.loads(xqueue_body['student_info'])
    xqueue_body['grader_payload'] = json.loads(xqueue_body['grader_payload'])
    return dict(xqueue_files=json.loads(content['xqueue_files']),
                xqueue_header=json.loads(content['xqueue_header']),
                xqueue_body=xqueue_body)


async def xqueue_putresult(http_client, config, xqueue_header, grade):
    url = config['xqueue'][
        'base_url'] + "/xqueue/put_result/?queue_name=" + config['xqueue'][
            'username']
    data = dict(xqueue_header=xqueue_header, xqueue_body=json.dumps(grade))
    body = await xqueue_post(http_client, url, data)
    if body['return_code'] != 0:
        raise XQueueException('xqueue putresult failed')


async def on_grader_message(message: aiormq.types.DeliveredMessage, config):
    logging.info("received amqp message:{}\n{}\n".format(
        message.delivery.routing_key, message.body))

    body = json.loads(message.body)
    result = yaml.load(body['yaml_result'], Loader=yaml.SafeLoader)
    grade = dict(correct=(result["grade"] == result["max-grade"]),
                 score=result["grade"],
                 msg=result["explanation"])
    xqueue_header = body['opaque']

    async with aiohttp.ClientSession() as session:
        await xqueue_login(session, config)
        await xqueue_putresult(session, config, xqueue_header, grade)

    await message.channel.basic_ack(message.delivery.delivery_tag)


async def process_submission(config, amqp_channel, submission):
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


async def poll_xqueue(config, amqp_channel):
    async with aiohttp.ClientSession() as session:
        await xqueue_login(session, config)
        queue_len = await xqueue_getqueuelen(session, config)
        logging.info("Polling xqueue ({} jobs submitted)".format(queue_len))
        if queue_len > 0:
            submission = await xqueue_getsubmission(session, config)
            await process_submission(config, amqp_channel, submission)
            queue_len -= 1
        return queue_len


async def main(config):
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
            await asyncio.sleep(2)


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
    loop = asyncio.get_event_loop()
    loop.run_until_complete(main(config))
