#! /bin/sh
#

docker build -t rfc1149/builder -f builder.dockerfile .
docker build -t rfc1149/gitlab-to-amqp -f gitlab-to-amqp.dockerfile .
