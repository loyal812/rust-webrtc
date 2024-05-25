use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Notify;
use tokio::time::Duration;
use arboard::Clipboard;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_H264, MIME_TYPE_OPUS};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_connection_state::RTCIceConnectionState;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::media::io::h264_reader::H264Reader;
use webrtc::media::io::ogg_reader::OggReader;
use webrtc::media::Sample;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;

#[tokio::main]
async fn main() -> Result<()> {
  let video_file = "stream_data/video_stream.h264";
  let audio_file = "stream_data/audio_stream.ogg";

  let mut m = MediaEngine::default();
  m.register_default_codecs()?;
  let mut registry = Registry::new();
  registry = register_default_interceptors(registry, &mut m)?;
  let api = APIBuilder::new()
    .with_media_engine(m)
    .with_interceptor_registry(registry)
    .build();
    
  let config = RTCConfiguration {
    ice_servers: vec![RTCIceServer {
      urls: vec!["stun:stun.l.google.com:19302".to_owned()],
      ..Default::default()
    }],
    ..Default::default()
  };
    
  let peer_connection = Arc::new(api.new_peer_connection(config).await?);

  let notify_tx = Arc::new(Notify::new());
  let notify_video = notify_tx.clone();
  let notify_audio = notify_tx.clone();

  let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(1);
  let video_done_tx = done_tx.clone();
  let audio_done_tx = done_tx.clone();

    let video_track = Arc::new(TrackLocalStaticSample::new(
      RTCRtpCodecCapability {
        mime_type: MIME_TYPE_H264.to_owned(),
        ..Default::default()
      },
      "video".to_owned(),
      "webrtc-rs".to_owned(),
    ));

    let rtp_sender = peer_connection
      .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>)
      .await?;

    tokio::spawn(async move {
      let mut rtcp_buf = vec![0u8; 1500];
      while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
      Result::<()>::Ok(())
    });

    let video_file_name = video_file.to_owned();
    tokio::spawn(async move {
        
      let file = File::open(&video_file_name)?;
      let reader = BufReader::new(file);
      let mut h264 = H264Reader::new(reader);
      notify_video.notified().await;

      println!("Playing video from disk file {video_file}");

      let mut ticker = tokio::time::interval(Duration::from_millis(33));
      loop {
        let nal = match h264.next_nal() {
          Ok(nal) => nal,
          Err(err) => {
            println!("All video frames parsed and sent: {err}");
            break;
          }
        };

        video_track
          .write_sample(&Sample {
            data: nal.data.freeze(),
            duration: Duration::from_secs(1),
            ..Default::default()
          })
          .await?;

        let _ = ticker.tick().await;
      }

      let _ = video_done_tx.try_send(());
      Result::<()>::Ok(())
    });
  
    let audio_track = Arc::new(TrackLocalStaticSample::new(
      RTCRtpCodecCapability {
        mime_type: MIME_TYPE_OPUS.to_owned(),
        ..Default::default()
      },
      "audio".to_owned(),
      "webrtc-rs".to_owned(),
    ));
      
    let rtp_sender = peer_connection
      .add_track(Arc::clone(&audio_track) as Arc<dyn TrackLocal + Send + Sync>)
      .await?;

    tokio::spawn(async move {
      let mut rtcp_buf = vec![0u8; 1500];
      while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
      Result::<()>::Ok(())
    });

    let audio_file_name = audio_file.to_owned();
    tokio::spawn(async move {
      let file = File::open(audio_file_name)?;
      let reader = BufReader::new(file);
      let (mut ogg, _) = OggReader::new(reader, true)?;
      notify_audio.notified().await;

      println!("Playing audio from disk file {audio_file}");

      let mut ticker = tokio::time::interval(Duration::from_millis(20));
      let mut last_granule: u64 = 0;
      while let Ok((page_data, page_header)) = ogg.parse_next_page() {
        let sample_count = page_header.granule_position - last_granule;
        last_granule = page_header.granule_position;
        let sample_duration = Duration::from_millis(sample_count * 1000 / 48000);
        audio_track
          .write_sample(&Sample {
            data: page_data.freeze(),
            duration: sample_duration,
            ..Default::default()
          })
          .await?;
        let _ = ticker.tick().await;
      }

      let _ = audio_done_tx.try_send(());

      Result::<()>::Ok(())
    });
    
  peer_connection.on_ice_connection_state_change(Box::new(
    move |connection_state: RTCIceConnectionState| {
      println!("Connection State has changed {connection_state}");
      if connection_state == RTCIceConnectionState::Connected {
        notify_tx.notify_waiters();
      }
      Box::pin(async {})
    },
  ));

  peer_connection.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
    println!("Peer Connection State has changed: {s}");

    if s == RTCPeerConnectionState::Failed {
      println!("Peer Connection has gone to failed exiting");
      let _ = done_tx.try_send(());
    }

    Box::pin(async {})
  }));

    
  let line = demo_webrtc::must_read_stdin()?;
  let desc_data = demo_webrtc::decode(line.as_str())?;
  let offer = serde_json::from_str::<RTCSessionDescription>(&desc_data)?;

  peer_connection.set_remote_description(offer).await?;
  let answer = peer_connection.create_answer(None).await?;
  let mut gather_complete = peer_connection.gathering_complete_promise().await;
  peer_connection.set_local_description(answer).await?;
  let _ = gather_complete.recv().await;

  if let Some(local_desc) = peer_connection.local_description().await {
    let json_str = serde_json::to_string(&local_desc)?;
    let b64 = demo_webrtc::encode(&json_str);
    if let Ok(_) = Clipboard::new()?.set_text(&b64) {
      println!("Copied description to clipboard");
    } else {
      println!("Failed to copy description to clipboard");
      println!("{b64}")
    }
  } else {
    println!("Generate local_description failed!");
  }

  tokio::select! {
    _ = done_rx.recv() => {
      println!("Received done signal!");
    }
    _ = tokio::signal::ctrl_c() => {
      println!();
    }
  };

  peer_connection.close().await?;

  Ok(())
}
