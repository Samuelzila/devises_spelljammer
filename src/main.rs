const CONFIG_PATH: &str = "./config";

use chrono::Local;
use num::{rational::Ratio, BigInt, BigRational, FromPrimitive, ToPrimitive};
use plotters::{
    prelude::*,
    style::full_palette::{BROWN, LIGHTBLUE, YELLOW_800},
};
use rand::Rng;
use resvg::usvg;
use serde::{Deserialize, Serialize};
use serenity::{
    all::{
        CommandInteraction, CommandOptionType, CreateAttachment, CreateCommand,
        CreateCommandOption, CreateInteractionResponse, CreateInteractionResponseMessage,
        EditMessage, GuildId, Interaction, PartialMember,
    },
    async_trait,
    model::{channel::Message, gateway::Ready},
    prelude::*,
};
use std::{cmp::Ordering, fs, path::Path, time::Duration};

#[derive(Serialize, Deserialize)]
struct Data {
    message: Message,
    data: [Vec<f64>; 5],
}
impl From<Message> for Data {
    fn from(message: Message) -> Self {
        Self {
            message,
            data: Default::default(),
        }
    }
}

struct Handler;
#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        //Update graph when bot loads.
        draw_graph().await;
        update_message(&ctx).await;

        println!("{} is connected!", ready.user.name);

        //Create commands for Hérauts de la lune déchue
        let motc = GuildId::new(636766648237555723);

        motc.create_command(
            &ctx.http,
            CreateCommand::new("currency_init").description("Initiate currency calculations"),
        )
        .await
        .unwrap();

        motc.create_command(
            &ctx.http,
            CreateCommand::new("currency_add_time")
                .description("Simulate a specified amout of days immediately")
                .add_option(CreateCommandOption::new(
                    CommandOptionType::Integer,
                    "time",
                    "Number of days to simulate.",
                )),
        )
        .await
        .unwrap();

        //Crate timer for generating data
        tokio::spawn(async move {
            loop {
                //Sleeping a few seconds will prevent the programm from executing this function twice in the same second
                tokio::time::sleep(Duration::from_secs(2)).await;
                //Convoluted way to find the time until midnight
                let time_until_midnight = (Local::now() + chrono::Duration::try_days(1).unwrap())
                    .date_naive()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .signed_duration_since(Local::now().naive_local())
                    .to_std()
                    .unwrap();

                tokio::time::sleep(time_until_midnight).await;

                //If config exists, add points to it
                if Path::new(CONFIG_PATH).exists() {
                    add_data(1).await;

                    draw_graph().await;

                    update_message(&ctx).await;
                }
            }
        });
    }

    //EventHandler for commands
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        //Check if interaction is a command
        if let Interaction::Command(command) = interaction {
            if command.data.name == "currency_init" {
                //Check if user is administrator
                if !is_admin(*command.member.clone().unwrap()) {
                    fak_you(&ctx, &command).await;
                } else {
                    //Only create config file if it doesn't already exist
                    if !Path::new(CONFIG_PATH).exists() {
                        //Initiate base values
                        let mut reference_terre: f64 = 1.0;
                        let mut reference_eau: f64 = 10.0;
                        let mut reference_feu: f64 = 50.0;
                        let mut reference_air: f64 = 200.0;
                        let mut reference_lum: f64 = 1.0;

                        //Sending first message and storing its id
                        command
                            .create_response(
                                &ctx.http,
                                CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .content("Generating data..."),
                                ),
                            )
                            .await
                            .unwrap();

                        let client_msg = command.get_response(&ctx.http).await.unwrap();

                        //Calculate the new values based on specified currency rate and specified chaos
                        reference_terre = calculate_rate(reference_terre, 1.0).await;
                        reference_air = calculate_rate(reference_air, 50.0).await;
                        reference_eau = calculate_rate(reference_eau, 3.0).await;
                        reference_feu = calculate_rate(reference_feu, 5.0).await;
                        reference_lum = calculate_rate(reference_lum, 1.0).await;

                        push_data(
                            (
                                reference_terre,
                                reference_air,
                                reference_eau,
                                reference_feu,
                                reference_lum,
                            ),
                            &client_msg,
                        )
                        .await;

                        draw_graph().await;

                        update_message(&ctx).await;
                    } else {
                        fak_you(&ctx, &command).await;
                    }
                }
            } else if command.data.name == "currency_add_time" {
                if !is_admin(*command.member.clone().unwrap()) {
                    fak_you(&ctx, &command).await;
                } else {
                    //Add simulated data for spcified amount of time.
                    add_data(command.data.options[0].value.as_i64().unwrap() as usize).await;

                    draw_graph().await;

                    update_message(&ctx).await;
                }
            }
        }
    }
}

async fn update_message(ctx: &Context) {
    let mut data = get_data().await;

    let reference_terre = *data.data[0].last().unwrap();
    let reference_air = *data.data[1].last().unwrap();
    let reference_eau = *data.data[2].last().unwrap();
    let reference_feu = *data.data[3].last().unwrap();
    let reference_lum = *data.data[4].last().unwrap();

    //Update discord message
    let new_message = EditMessage::new().content(
        format!("Terre: {:.5}\nAir: {:.5}\nEau: {:.5}\nFeu: {:.5}\nLumière: {:.5}\nValeur de référence (demi-miche de pain): {:.5}\n", 1.0, reference_air/reference_terre, reference_eau/reference_terre, reference_feu/reference_terre, reference_lum/reference_terre, 1.0/reference_terre)
    )
    .remove_all_attachments()
    .new_attachment(CreateAttachment::path("./images/graph.png").await.expect("Could not attach graph."));

    data.message.edit(&ctx.http, new_message).await.unwrap();
}

async fn add_data(amount: usize) {
    //If config exists, add points to it
    if Path::new(CONFIG_PATH).exists() {
        let mut data = get_data().await;

        let mut reference_terre = *data.data[0].last().unwrap();
        let mut reference_air = *data.data[1].last().unwrap();
        let mut reference_eau = *data.data[2].last().unwrap();
        let mut reference_feu = *data.data[3].last().unwrap();
        let mut reference_lum = *data.data[4].last().unwrap();

        //Simulate for specified amount of time.
        for _ in 0..amount {
            //Calculate the new values based on specified currency rate and specified chaos
            reference_terre = calculate_rate(reference_terre, 1.0).await;
            reference_air = calculate_rate(reference_air, 50.0).await;
            reference_eau = calculate_rate(reference_eau, 3.0).await;
            reference_feu = calculate_rate(reference_feu, 5.0).await;
            reference_lum = calculate_rate(reference_lum, 1.0).await;

            data.data[0].push(reference_terre);
            data.data[1].push(reference_air);
            data.data[2].push(reference_eau);
            data.data[3].push(reference_feu);
            data.data[4].push(reference_lum);
        }

        fs::write(CONFIG_PATH, serde_json::to_string_pretty(&data).unwrap())
            .expect("Could not write to file.");
    }
}

/**
Store currency values in file using a tupple (Terre, Air, Eau, Feu, Lumière)
*/
async fn push_data(data: (f64, f64, f64, f64, f64), msg: &Message) {
    let mut config;
    if Path::new(CONFIG_PATH).exists() {
        config = get_data().await;
    } else {
        config = Data::from(msg.clone());
    }

    config.data[0].push(data.0);
    config.data[1].push(data.1);
    config.data[2].push(data.2);
    config.data[3].push(data.3);
    config.data[4].push(data.4);

    fs::write(CONFIG_PATH, serde_json::to_string_pretty(&config).unwrap())
        .expect("Could not write to file.");
}

#[inline]
fn is_admin(member: impl Into<PartialMember>) -> bool {
    member.into().permissions.unwrap().administrator()
}

/*
Sends a permission denied error to the channel.
*/
#[inline]
async fn fak_you(ctx: &Context, command: &CommandInteraction) {
    let builder = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .add_file(CreateAttachment::path("./fak_you.mp4").await.unwrap()),
    );

    command.create_response(&ctx.http, builder).await.unwrap();
}

async fn get_data() -> Data {
    serde_json::from_slice(&fs::read(CONFIG_PATH).expect("Unable to read file.")).unwrap()
}

/**
Calculate new value for currency by passing its current value and a chaos value.
*/
async fn calculate_rate(value: f64, chaos: f64) -> f64 {
    //The rate is the increase in value per day, in percentage.
    let rate = to_big_rationnal(0.15) / to_big_rationnal(365.2422);
    //x corresponds to the rate plus the chaos per day. The bigger either value is, the greater the
    //fluctuation will be. the chaos behaves similarly to a standard deviation.
    let x = to_big_rationnal(chaos) / to_big_rationnal(36524.22) + &rate;
    //y is the percentage that "cancels out" x's chaos part in an exponential function.
    //If we were to multiply a value by 1 + x and then by 1 + y, the result would be the value multiplied by the rate.
    //x and y are in a way the upper and lower bounds, respectively, of our chaos.
    let y =
        ((to_big_rationnal(1.0) + &rate) / (to_big_rationnal(1.0) + &x)) - to_big_rationnal(1.0);

    //We generate a number between 0 and (x - y), then add y. Effectively, our random number is between y and x.
    let random = to_big_rationnal(rand::thread_rng().gen_range(0.0..1.0)) * (&x - &y) + &y;

    //This is just a one step exponential function with our random number as a parameter.
    (to_big_rationnal(value) * (to_big_rationnal(1.0) + (random)))
        .to_f64()
        .unwrap()
}

fn find_max(x: &Vec<f64>) -> f64 {
    *x.iter()
        .max_by(|a, b| {
            if a.max(**b) == **a {
                Ordering::Greater
            } else if a.max(**b) == **b {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        })
        .unwrap()
}

fn find_min(x: &Vec<f64>) -> f64 {
    *x.iter()
        .min_by(|a, b| {
            if a.max(**b) == **a {
                Ordering::Greater
            } else if a.max(**b) == **b {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        })
        .unwrap()
}

#[inline]
/**
Convert float 64 to BigRationnal
*/
fn to_big_rationnal(x: f64) -> Ratio<BigInt> {
    BigRational::from_f64(x).unwrap()
}

async fn draw_graph() {
    //Scoping here allows the graph to be dropped and the svg file created before we call render_svg.
    {
        let mut data = get_data().await;

        let root_drawing_area =
            SVGBackend::new("./images/graph.svg", (1440, 1080)).into_drawing_area();

        root_drawing_area.fill(&WHITE).unwrap();

        let vec_size = data.data[0].len() as f64;

        if vec_size > 28.0 {
            data.data[0] = data.data[0]
                .drain((&data.data[0].len() - 28)..data.data[0].len())
                .collect();
            data.data[2] = data.data[2]
                .drain((&data.data[2].len() - 28)..data.data[2].len())
                .collect();
            data.data[3] = data.data[3]
                .drain((&data.data[3].len() - 28)..data.data[3].len())
                .collect();
            data.data[1] = data.data[1]
                .drain((&data.data[1].len() - 28)..data.data[1].len())
                .collect();
            data.data[4] = data.data[4]
                .drain((&data.data[4].len() - 28)..data.data[4].len())
                .collect();
        }

        let max_terre = find_max(&data.data[0]);
        let max_eau = find_max(&data.data[2]) / 10.0;
        let max_feu = find_max(&data.data[3]) / 50.0;
        let max_air = find_max(&data.data[1]) / 200.0;
        let max_lum = find_max(&data.data[4]);

        let min_terre = find_min(&data.data[0]);
        let min_eau = find_min(&data.data[2]) / 10.0;
        let min_feu = find_min(&data.data[3]) / 50.0;
        let min_air = find_min(&data.data[1]) / 200.0;
        let min_lum = find_min(&data.data[4]);

        let max = max_terre.max(max_eau.max(max_feu.max(max_air.max(max_air.max(max_lum)))));
        let min = min_terre.min(min_eau.min(min_feu.min(min_air.min(min_air.min(min_lum)))));

        //Find where to start the graph on x axis
        let start;
        if vec_size < 28.0 {
            start = 0.0
        } else {
            start = vec_size - 28.0
        }

        //Index useful for iterators
        let mut index;
        let mut chart = ChartBuilder::on(&root_drawing_area)
            .set_left_and_bottom_label_area_size(60)
            .margin_right(60)
            .margin_top(60)
            .build_cartesian_2d(start..(vec_size - 1.0), min..max)
            .unwrap();

        chart
            .configure_mesh()
            .label_style(("sans-serif", 20))
            .y_label_formatter(&|y| format!("{:.4}", y))
            //Set the amount of grid lines as the smallest number between 28 and the amount of integers in the domain.
            .x_labels(28_f64.min(vec_size) as usize)
            //Remove decimals from x axis
            .x_label_formatter(&|x| format!("{:.0}", x))
            //remove minor grid
            .max_light_lines(0)
            .draw()
            .unwrap();

        index = start - 1.0;
        chart
            .draw_series(LineSeries::new(
                (data.data[0].iter()).map(|x| {
                    index += 1.0;
                    (index, *x)
                }),
                &BROWN,
            ))
            .unwrap()
            .label("Terre")
            .legend(|(x, y)| Circle::new((x, y), 5, BROWN.filled()));

        index = start - 1.0;
        chart
            .draw_series(LineSeries::new(
                (data.data[1].iter()).map(|x| {
                    index += 1.0;
                    (index, *x / 200.0)
                }),
                &LIGHTBLUE,
            ))
            .unwrap()
            .label("Air")
            .legend(|(x, y)| Circle::new((x, y), 5, LIGHTBLUE.filled()));

        index = start - 1.0;
        chart
            .draw_series(LineSeries::new(
                (data.data[2].iter()).map(|x| {
                    index += 1.0;
                    (index, *x / 10.0)
                }),
                &BLUE,
            ))
            .unwrap()
            .label("Eau")
            .legend(|(x, y)| Circle::new((x, y), 5, BLUE.filled()));

        index = start - 1.0;
        chart
            .draw_series(LineSeries::new(
                (data.data[3].iter()).map(|x| {
                    index += 1.0;
                    (index, *x / 50.0)
                }),
                &RED,
            ))
            .unwrap()
            .label("Feu")
            .legend(|(x, y)| Circle::new((x, y), 5, RED.filled()));

        index = start - 1.0;
        chart
            .draw_series(LineSeries::new(
                (data.data[4].iter()).map(|x| {
                    index += 1.0;
                    (index, *x)
                }),
                &YELLOW_800,
            ))
            .unwrap()
            .label("Lumière/Ténèbre")
            .legend(|(x, y)| Circle::new((x, y), 5, YELLOW_800.filled()));

        chart
            .configure_series_labels()
            .border_style(&BLACK)
            .background_style(&WHITE.mix(0.8))
            .label_font(("sans-serif", 25))
            .draw()
            .unwrap();
    }

    //Convert SVG image to PNG
    render_svg();
}

/**
Very ugly function to render svg to png
*/
fn render_svg() {
    let mut fontdb = usvg::fontdb::Database::default();
    fontdb.load_system_fonts();
    let tree = usvg::Tree::from_data(
        &fs::read("./images/graph.svg").unwrap(),
        &usvg::Options::default(),
        &fontdb,
    )
    .unwrap();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(1440, 1080).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    pixmap.save_png("./images/graph.png").unwrap();
}

#[tokio::main]
async fn main() {
    //Initialise environment variables.
    let token = dotenv::var("TOKEN").expect("Could not read environment variables.");

    //Starting discord client
    let mut client = Client::builder(token, GatewayIntents::from(GatewayIntents::all()))
        .event_handler(Handler)
        .await
        .expect("Error creating client.");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
