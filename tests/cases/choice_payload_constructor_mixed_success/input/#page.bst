#Response :: Success, Pending | retry_count Int, message String |;

p = Response::Pending(3, message = "waiting")
io("ok")
