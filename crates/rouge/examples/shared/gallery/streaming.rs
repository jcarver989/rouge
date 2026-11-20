use super::{Cell, band, cell, section};
use rouge::core::view;
use rouge::core::{Div, Styled, div};
use rouge::markdown::MarkdownStream;
use rouge::reactive::{Signal, keyed_list, signal, sleep, spawn};
use rouge::widgets::{Message, composer, transcript};
use std::time::Duration;

const REPLY: &str = "Try **Soft Machinery** by Ines Okafor.\n\n\
    - *Glasswork* opens slowly\n\
    - *Harmattan* is the quiet one\n\n\
    It runs 43 minutes, so it will see you through.";

pub(crate) fn streaming(status: Signal<String>) -> Div {
    section(
        "Streaming",
        "A transcript on virtual_list that follows its end, markdown that re-parses only the open block, and a composer whose Send button morphs into Stop while streaming.",
        [band([chat(status)])],
    )
}

fn chat(status: Signal<String>) -> Cell {
    let thread = keyed_list(|(id, _): &(usize, Message)| *id);
    let opening = signal(MarkdownStream::new());
    opening.update(|stream| {
        stream.push("Ask for something to play and the reply streams in here.");
        stream.finish();
    });
    thread.push((0, Message::assistant(opening)));
    let (draft, busy, generation) = (signal(String::new()), signal(false), signal(0usize));
    let send = move |said: String| {
        let reply = signal(MarkdownStream::new());
        thread.push((thread.len(), Message::user(said)));
        thread.push((thread.len(), Message::assistant(reply)));
        let turn = generation.get_untracked() + 1;
        generation.set(turn);
        busy.set(true);
        spawn(async move {
            for word in REPLY.split_inclusive(' ') {
                sleep(Duration::from_millis(40)).await;
                if generation.get_untracked() != turn {
                    reply.update(|stream| {
                        stream.finish();
                    });
                    return;
                }
                reply.update(|stream| {
                    stream.push(word);
                });
            }
            reply.update(|stream| {
                stream.finish();
            });
            if generation.get_untracked() == turn {
                busy.set(false);
                status.set("Reply finished".into());
            }
        });
    };
    let stop = move || {
        generation.update(|turn| *turn += 1);
        busy.set(false);
        status.set("Reply stopped".into());
    };
    let content = view! {
        <div flex col gap=12.0 h=320.0>
            <transcript messages=thread message={|(_, message)| message.clone()} test_id="thread" flex_1/>
            <composer draft=draft test_id="composer" placeholder="Ask for something to play" busy=busy
                 on_send=send on_stop=stop/>
        </div>
    };
    cell(
        "transcript · composer",
        1.0,
        "While a reply streams the composer holds its draft and Send morphs into a circular Stop control.",
        content,
    )
}
