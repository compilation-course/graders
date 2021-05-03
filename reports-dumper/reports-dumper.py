#! /usr/bin/env python
#
# Usage: reports-dumper.py config-file results-file.csv

import csv
import pika
import sys
import yaml

amqp_conf = yaml.safe_load(open(sys.argv[1], newline = ""))["amqp"]
if "reports_routing_key" not in amqp_conf:
    sys.stderr.write("No reports_routing_key in configuration file.\n")
    sys.exit(1)

try:
    results = csv.DictReader(open(sys.argv[2]))
    results = dict((row["name"], row) for row in results)
    for d in results.values():
        for (k, v) in d.items():
            try: d[k] = float(v)
            except ValueError: pass
except FileNotFoundError:
    results = {}

def add_result(name, lab, grade, max_grade):
    r = results.get(name, {"name": name})
    p = round(1000 * grade / max_grade) / 10
    r["{} (latest %)".format(lab)] = p
    h = "{} (highest %)".format(lab)
    r[h] = max(p, r.get(h) or 0)
    results[name] = r
    print(name, r)
    dump_results()

def dump_results():
    fields = set()
    for r in results.values():
        fields = fields.union(r)
        for k in r.keys(): fields.add(k)
    fields.remove("name")
    fields = ["name"] + sorted(fields)
    with open(sys.argv[2], "w", newline = "") as fd:
        w = csv.DictWriter(fd, fields)
        w.writeheader()
        for r in sorted(results.values(), key=lambda x: x["name"]):
            w.writerow(r)

def callback(ch, method, properties, body):
    body = yaml.safe_load(body)
    lab = body["lab"]
    opaque = yaml.safe_load(body["opaque"])
    repository = opaque[0]["repository"]["name"]
    result = yaml.safe_load(body["yaml_result"])
    grade = result["grade"]
    max_grade = result["max-grade"]
    add_result(repository, lab, grade, max_grade)
    ch.basic_ack(method.delivery_tag)

connection = pika.BlockingConnection(pika.ConnectionParameters(amqp_conf["host"], amqp_conf["port"]))
channel = connection.channel()
channel.queue_declare(amqp_conf["reports_routing_key"], durable = True)
channel.basic_consume(queue=amqp_conf["reports_routing_key"], on_message_callback=callback)
channel.start_consuming()
