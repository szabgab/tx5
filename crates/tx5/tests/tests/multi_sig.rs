use std::sync::Arc;

async fn sbd() -> sbd_server::SbdServer {
    let config = sbd_server::Config {
        bind: vec!["127.0.0.1:0".to_string(), "[::1]:0".to_string()],
        limit_clients: 100,
        disable_rate_limiting: true,
        ..Default::default()
    };
    sbd_server::SbdServer::new(Arc::new(config)).await.unwrap()
}

async fn ep(
    s: &sbd_server::SbdServer,
) -> (tx5::PeerUrl, tx5::Endpoint, tx5::EndpointRecv) {
    let config = tx5::Config {
        signal_allow_plain_text: true,
        ..Default::default()
    };
    let (ep, recv) = tx5::Endpoint::new(Arc::new(config));
    let sig = format!("ws://{}", s.bind_addrs()[0]);
    let peer_url = ep.listen(tx5::SigUrl::parse(sig).unwrap()).await.unwrap();
    (peer_url, ep, recv)
}

async fn check_msg(
    msg: &str,
    l_addrs: &mut Vec<tx5::PeerUrl>,
    r: &mut tx5::EndpointRecv,
) -> tx5::PeerUrl {
    loop {
        let evt = r.recv().await;
        println!("{evt:?}");
        match evt {
            None => panic!("unexpected end of receiver"),
            Some(tx5::EndpointEvent::ListeningAddressOpen { local_url }) => {
                l_addrs.push(local_url);
            }
            Some(tx5::EndpointEvent::Message { peer_url, message }) => {
                let message = String::from_utf8_lossy(&message);
                assert_eq!(msg, message);
                return peer_url;
            }
            _ => (),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn multi_sig() {
    let mut l_addrs = Vec::new();

    let sig1 = sbd().await;
    let (p1, e1, mut r1) = ep(&sig1).await;

    let sig2 = sbd().await;
    let (p2, e2, mut r2) = ep(&sig2).await;

    // make sure we can message each other at their listening addrs

    e1.send(p2.clone(), b"hello".to_vec()).await.unwrap();

    let p1oth = check_msg("hello", &mut l_addrs, &mut r2).await;

    assert_ne!(p1, p1oth);
    assert_ne!(p2, p1oth);

    e2.send(p1.clone(), b"world".to_vec()).await.unwrap();

    let p2oth = check_msg("world", &mut l_addrs, &mut r1).await;

    assert_ne!(p1, p2oth);
    assert_ne!(p2, p2oth);

    // make sure we can also message each other at the new connection addrs

    e1.send(p2oth, b"foo".to_vec()).await.unwrap();

    let _ = check_msg("foo", &mut l_addrs, &mut r2).await;

    e2.send(p1oth, b"bar".to_vec()).await.unwrap();

    let _ = check_msg("bar", &mut l_addrs, &mut r1).await;

    // make sure we only ever emitted one listening event per endpoint
    assert_eq!(2, l_addrs.len());
}
