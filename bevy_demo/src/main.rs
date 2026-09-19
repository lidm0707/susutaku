use bevy::prelude::*;

fn hello() {
    println!("Hello from headless Bevy!");
}

fn tick(mut frames: Local<u32>, mut exit: EventWriter<AppExit>) {
    *frames += 1;
    println!("frame {}", *frames);
    if *frames >= 3 {
        exit.send(AppExit);
    }
}

fn main() {
    App::new()
        .add_systems(Startup, hello)
        .add_systems(Update, tick)
        .run();
}
