use futures::Future;
use futures::stream::Stream

pub struct Message {

}

impl Message {
    pub fn readAll (&self) -> impl Future {
        let acc = [];
        this.on('data', (d) => acc.push(d));
        this.on('end', () => {
            if (this.error)
            return callback('transport', this.error, null);
            if (this.header.error_code)
            return callback(this.header.error_code, this.header.error, null);

            var resp = Buffer.concat(acc).toString('utf8');
            try {
                resp = JSON.parse(resp);
            } catch (e) {}

            if (resp === undefined)
            return callback('transport', 'failed to parse JSON response', null);

            return callback(null, null, resp);
        });
    }
}

impl Stream for Message {
    type Item = u64;

    // The stream will never yield an error
    type Error = ();

    fn poll(&mut self) -> Poll<Option<u64>, ()> {
        let curr = self.curr;
        let next = curr + self.next;

        self.curr = self.next;
        self.next = next;

        Ok(Async::Ready(Some(curr)))
    }
}
