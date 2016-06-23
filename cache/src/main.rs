#![feature(custom_derive)]
extern crate nix;
extern crate env_logger;
extern crate threadpool;
extern crate ws;
extern crate num_cpus;
#[macro_use]
extern crate log;
#[macro_use]
extern crate libsonicd;

use libsonicd::*;
use std::sync::mpsc::channel;
use std::option::Option;
use nix::sys::epoll::*;
use std::sync::mpsc;
use std::rc::Rc;
use std::cell::Cell;
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::{thread, fmt, net, marker, slice};
// use nix::c_int;
// use nix::fcntl::{O_NONBLOCK, O_CLOEXEC};
// use nix::errno::{EWOULDBLOCK, EINPROGRESS};
use nix::sys::socket::*;
use nix::unistd;
use std::os::raw::c_int;
use std::os::unix::io::{RawFd, AsRawFd};

static VERSION: &'static str = env!("CARGO_PKG_VERSION");
static COMMIT: Option<&'static str> = option_env!("SONICD_COMMIT");

// FIXME this is WIP

// TODO https://github.com/BurntSushi/chan-signal
// use nix::sys::signal;
fn ssig() {}

macro_rules! perror {
    ($m:expr) => {{
        |e: nix::Error| {
            error!("{}: {}", $m, e);
            std::process::exit(1);
        }
    }}
}

macro_rules! eagain {
    ($syscall:expr, $name:expr, $arg1: expr, $arg2: expr) => {{
        let mut res = None;
        loop {
            match $syscall($arg1, $arg2) {
                Err(nix::Error::Sys(a@nix::Errno::EAGAIN)) => {
                    debug!("{}: {}", $name, a);
                    continue;
                },
                Ok(m) => {
                    res = Some(m);
                    break;
                }
                Err(e) => {
                    perror!(format!("{}", $name))(e);
                    break;
                }
            }
        }
        res.unwrap()
    }}
}

#[inline]
fn ginterest(fd: RawFd) -> EpollEvent {
    EpollEvent {
        events: EPOLLET | EPOLLIN | EPOLLOUT | EPOLLERR | EPOLLRDHUP,
        data: fd as u64,
    }
}

fn main() {

    ssig();

    env_logger::init().unwrap();

    let c = COMMIT.unwrap_or_else(|| "dev");
    info!("starting sonicd cache v.{} ({})", VERSION, c);

    let addr = SockAddr::Inet(InetAddr::from_std(&("127.0.0.1:10001".parse().unwrap())));
    let max_conn = 1024;

    let sockf = SOCK_CLOEXEC | SOCK_NONBLOCK;
    let protocol = 0;
    let srvfd = socket(AddressFamily::Inet, SockType::Stream, sockf, protocol).unwrap() as i32;
    // setsockopt(srvfd, SOL_SOCKET, &1).unwrap();

    eagain!(bind, "bind", srvfd, &addr);
    debug!("bind: success fd {} to {}", srvfd, addr);

    eagain!(listen, "listen", srvfd, max_conn);
    debug!("listen: success fd {}: {}", srvfd, addr);

    let cpus = num_cpus::get();
    debug!("machine has {} (v)cpus", cpus);

    let mut epfds: Vec<RawFd> = Vec::with_capacity(cpus);

    // epoll_wait with no timeout
    let loop_ms = -1 as isize;

    let ninterest: EpollEvent = {
        EpollEvent {
            events: EpollEventKind::empty(),
            data: 0,
        }
    };


    for _ in 1..cpus {

        let epfd = epoll_create().unwrap_or_else(perror!("epoll_create"));
        epfds.push(epfd);

        thread::spawn(move || {

            loop {

                let mut evts: Vec<EpollEvent> = Vec::with_capacity(max_conn);
                let dst = unsafe { slice::from_raw_parts_mut(evts.as_mut_ptr(), evts.capacity()) };
                let cnt = epoll_wait(epfd, dst, loop_ms).unwrap_or_else(perror!("epoll_wait"));
                unsafe { evts.set_len(cnt) }

                for ev in evts {
                    let fd: c_int = ev.data as i32;

                    let epoll = ev.events;
                    if epoll.contains(EPOLLRDHUP) {
                        shutdown(fd, Shutdown::Both).unwrap();
                        debug!("successfully closed fd {}", fd);

                        epoll_ctl(epfd, EpollOp::EpollCtlDel, fd, &ninterest)
                            .unwrap_or_else(perror!("epoll_ctl"));
                        debug!("unregistered interests for {}", fd);
                        continue;
                    }

                    if epoll.contains(EPOLLERR) {
                        error!("socket error on {}", fd);
                    }

                    if epoll.contains(EPOLLIN) {
                        debug!("socket of {} is readable", fd);

                        // FIXME 
                        // main loop should be connection handling
                        // once new connection is accepted, a new handler should be created with
                        // a channel that will be used to communicate with epoll instance
                        
                        // TODO setup handler and store
                        // let msg = libsonicd::read_message(&fd);
                        // tx.send(msg).unwrap();
                    }

                    if epoll.contains(EPOLLOUT) {
                        debug!("socket of {} is writable", fd);
                    }
                }
            }

        });
    }

    let mut accepted: u64 = 0;

    let cepfd = epoll_create().unwrap_or_else(perror!("epoll_create"));

    let io_cpus = cpus - 1;

    let info = ginterest(srvfd);
    epoll_ctl(cepfd, EpollOp::EpollCtlAdd, srvfd, &info).unwrap_or_else(perror!("epoll_ctl"));

    thread::spawn(move || {

        loop {
            let mut evts: Vec<EpollEvent> = Vec::with_capacity(max_conn);
            let dst = unsafe { slice::from_raw_parts_mut(evts.as_mut_ptr(), evts.capacity()) };
            // Wait for epoll events for at most timeout_ms milliseconds
            let cnt = epoll_wait(cepfd, dst, loop_ms).unwrap_or_else(perror!("epoll_wait"));
            unsafe { evts.set_len(cnt) }

            for _ in evts {

                let clifd = eagain!(accept4, "accept4", srvfd, sockf);
                debug!("accept4: accpeted new tcp client {}", &clifd);

                let info = ginterest(clifd);

                // round robin
                let next = (accepted % io_cpus as u64) as usize;

                let epfd: RawFd = *epfds.get(next).unwrap();

                debug!("assigned client to next {} epoll instance {}", &next, &epfd);

                epoll_ctl(epfd, EpollOp::EpollCtlAdd, clifd, &info)
                    .unwrap_or_else(perror!("epoll_ctl"));

                debug!("epoll_ctl: registered interests for {}", clifd);

                accepted += 1;
            }
        }
    });
    // loop {
    //    let msg = rx.recv().unwrap().unwrap();
    //    debug!("recv message {:?}", msg);
    // }
}
// ::ws::listen("127.0.0.1:9111", |out| WsHandler::new(out, count.clone())) .unwrap();
// use threadpool::ThreadPool;

// let n_workers = 4;
// let n_jobs = 8;
// let pool = ThreadPool::new(n_workers);

// let (tx, rx) = channel();

// for i in 0..n_jobs {
//    let tx = tx.clone();

//    pool.execute(move || {
//        tx.send(i).unwrap();
//    });
// }

// let count = Rc::new(Cell::new(0));

// assert_eq!(rx.iter().take(n_jobs).fold(0, |a, b| a + b), 28);
