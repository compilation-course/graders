// Example from edX.org
// {
//   "xqueue_header": {
//     "submission_id": 12,
//     "submission_key": "280587728458c29e1e66ae0c54a806f4"
//   }
//   "xqueue_files": {
//     "helloworld.c": "http://download.location.com/helloworld.c"
//   }
//   "xqueue_body":
//   "{
//     "student_info": {
//       "anonymous_student_id": "106ecd878f4148a5cabb6bbb0979b730",
//       "submission_time": "20160324104521",
//       "random_seed": 334
//     },
//     "student_response": "def double(x):\n return 2*x\n",
//     "grader_payload": "problem_2"
//    }"
// }

use futures::{Future, IntoFuture};
use hyper::{Client, StatusCode};
use tokio_core::reactor::Handle;

struct XQueue {
    base_url: String,
    login: String,
    password: String,
}

impl XQueue {
    fn login(&mut self, handle: &Handle) -> Box<Future<Item = bool, Error = ::errors::Error>> {
        let client = Client::new(handle);
        let url = format!("{}/xqueue/login", self.base_url)
            .parse()
            .into_future()
            .map_err(|e| ::errors::Error::from(e));
        Box::new(
            url.and_then(move |url| client.get(url).map_err(|e| e.into()))
                .map(|res| res.status() == StatusCode::Ok),
        )
    }
}
