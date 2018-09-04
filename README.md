# The global picture

A hook on Gitlab triggers at every push and is sent via https (or http depending on the Gitlab configuration)
to `gitlab-to-amqp` (note: it may require a front-end such as `nginx` to handle SSL).

`gitlab-to-amqp` does the following:

- It sets the status on the commit on Gitlab to mark that work on this commit is in progress.
- It clones the repository using credentials transmitted in the hook payload.
- It checks the repository for the presence of enabled labs using directory checks and file existence.
- It builds a zip file for every valid lab in the repository and is ready to serve it through http.
- It signals AMQP for the job existence by sending metadata (in JSON format) and the zip file URL.
- Later, when it gets the diagnostic of this job run, it removes the corresponding zip file
  from the file system and posts the diagnostic to Gitlab using its configured credentials.

A configurable number of zip processes can occur in parallel. The choice of zip archives
has been made to facilitate the replacement of `gitlab-to-amqp` by `xqueue-to-amqp` when
the MOOC and its labs will be deployed to edX.

One or more instances of `amqp-to-test` will take care of running the tests. For every job:

- The AMQP message is not acknowledged so that it gets redelivered if this instance of
  `amqp-to-test` fails.
- A `builder` docker is started using parameters pertaining to the particular lab which must be run.
- The docker is passed the lab name, arguments, URL of the zip file, and name of the expected
  top-level directory inside the zip file (to prevent accidental or voluntary zip bombs).
- Inside the docker, `test.py` is used to run tests according to the YAML description of the
  lab. It outputs a YAML diagnostic, and if `test.py` cannot be run, the `builder` will generate
  a failure diagnostic instead.
- The `builder` docker outputs its YAML diagnostic on standard output, and `amqp-to-test`
  posts it to the appropriate response queue.
- The job is acknowledged in AMQP so that it does not get resubmitted.

```
                      (git clone & zip)                           (unzip+build)
gitlab ---hook--> gitlab-to-amqp ---amqp--> amqp-to-test --docker--> builder --subprocess--> test.py
 ^                  |  ^  ^                     | |   ^                 | (if error)            |
 |                  |  |  |                     | |   |                 v                       |
 +-http (report)----+  |  +---- http (get zip)--+ |   +-----------------yaml--------------------+
                       |                          |
                       +---------amqp-------------+
```

# Notes

## Security

Everything assumes that except for the gitlab hook entry point, everything is present on a private
network. For example, the AMQP instance should only be accessible to `gitlab-to-amqp` and the
`amqp-to-test` instances. Also, the file serving part of `gitlab-to-amqp` uses http to serve
zip files (whose name is generated randomly, so exposing the endpoint is not problematic
as long as the name cannot be extracted from AMQP).

## Installing an AMQP server

An AMQP server that does not require installation or configuration can be started as-is:

``` bash
$ docker run -d --restart=unless-stopped -p 5672:5672 --name=rabbitmq rabbitmq
```
