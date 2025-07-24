const CONFIG_PATH: &str = "./config";

use chrono::Local;
use num::{rational::Ratio, BigInt, BigRational, FromPrimitive, ToPrimitive};
use plotters::{
    data,
    prelude::*,
    style::full_palette::{BROWN, LIGHTBLUE, YELLOW_800},
};
use rand::Rng;
use resvg::usvg;
use rusqlite as sql;
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
use std::{cmp::Ordering, fs, io, path::Path, time::Duration};

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

// Initialize SQLite database
fn init_db() -> sql::Result<sql::Connection> {
    let conn = sql::Connection::open("./currency.sqlite3")?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS currency_data (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            terre REAL NOT NULL,
            air REAL NOT NULL,
            eau REAL NOT NULL,
            feu REAL NOT NULL,
            lumiere REAL NOT NULL
        )",
        [],
    )?;

    // Insert initial data if the table is empty
    conn.execute(
        "INSERT INTO currency_data (terre, air, eau, feu, lumiere) 
         SELECT 1.0, 200.0, 10.0, 50.0, 1.0 
         WHERE NOT EXISTS (SELECT 1 FROM currency_data)",
        [],
    )?;

    Ok(conn)
}

struct Handler;
#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        //Update graph when bot loads.
        if Path::new(CONFIG_PATH).exists() {
            draw_graph().await.unwrap();
            update_message(&ctx).await.unwrap();
        }

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
                //Convoluted way to find the time until 4 am
                let time_until_midnight = (Local::now() + chrono::Duration::try_days(1).unwrap())
                    .date_naive()
                    .and_hms_opt(4, 0, 0)
                    .unwrap()
                    .signed_duration_since(Local::now().naive_local())
                    .to_std()
                    .unwrap();

                tokio::time::sleep(time_until_midnight).await;

                //If config exists, add points to it
                if Path::new(CONFIG_PATH).exists() {
                    add_data(1).await.unwrap();

                    draw_graph().await.unwrap();

                    update_message(&ctx).await.unwrap();
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

                        //Store message in config file
                        let data: Data = client_msg.into();
                        write_file(CONFIG_PATH, serde_json::to_string(&data).unwrap())
                            .expect("Could not write config file.");

                        draw_graph().await.unwrap();

                        update_message(&ctx).await.unwrap();
                    } else {
                        fak_you(&ctx, &command).await;
                    }
                }
            } else if command.data.name == "currency_add_time" {
                if !is_admin(*command.member.clone().unwrap()) {
                    fak_you(&ctx, &command).await;
                } else {
                    //Add simulated data for spcified amount of time.
                    add_data(command.data.options[0].value.as_i64().unwrap() as usize)
                        .await
                        .unwrap();

                    draw_graph().await.unwrap();

                    update_message(&ctx).await.unwrap();
                }
            }
        }
    }
}

async fn get_config() -> Result<Data, io::Error> {
    //Read config file
    let contents = fs::read_to_string(CONFIG_PATH)?;
    //Parse config file
    let data: Data = serde_json::from_str(&contents)?;

    Ok(data)
}

async fn update_message(ctx: &Context) -> Result<(), Box<dyn std::error::Error>> {
    let data = get_data().await?;
    let mut message = get_config().await?.message;

    let last = data.last().unwrap();

    let reference_terre = last.terre;
    let reference_air = last.air;
    let reference_eau = last.eau;
    let reference_feu = last.feu;
    let reference_lum = last.lumiere;

    //Update discord message
    let new_message = EditMessage::new().content(
        format!("Terre: {:.5}\nAir: {:.5}\nEau: {:.5}\nFeu: {:.5}\nLumière: {:.5}\nValeur de référence (demi-miche de pain): {:.5}\n", 1.0, reference_air/reference_terre, reference_eau/reference_terre, reference_feu/reference_terre, reference_lum/reference_terre, 1.0/reference_terre)
    )
    .remove_all_attachments()
    .new_attachment(CreateAttachment::path("./images/graph.png").await.expect("Could not attach graph."));

    message.edit(&ctx.http, new_message).await.unwrap();

    Ok(())
}

async fn add_data(amount: usize) -> Result<(), sql::Error> {
    let data: Vec<CurrencyRow> = get_data().await?;
    let last = data.last().unwrap();

    let mut reference_terre = last.terre;
    let mut reference_air = last.air;
    let mut reference_eau = last.eau;
    let mut reference_feu = last.feu;
    let mut reference_lum = last.lumiere;

    //Simulate for specified amount of time.
    for _ in 0..amount {
        //Calculate the new values based on specified currency rate and specified chaos
        reference_terre = calculate_rate(reference_terre, 1.0).await;
        reference_air = calculate_rate(reference_air, 50.0).await;
        reference_eau = calculate_rate(reference_eau, 3.0).await;
        reference_feu = calculate_rate(reference_feu, 5.0).await;
        reference_lum = calculate_rate(reference_lum, 1.0).await;

        push_data((
            reference_terre,
            reference_air,
            reference_eau,
            reference_feu,
            reference_lum,
        ))
        .await?
    }

    Ok(())
}

/**
Store currency values in file using a tupple (Terre, Air, Eau, Feu, Lumière)
*/
async fn push_data(data: (f64, f64, f64, f64, f64)) -> Result<(), sql::Error> {
    let conn = init_db()?;

    conn.execute(
        "INSERT INTO currency_data (terre, air, eau, feu, lumiere) VALUES (?1, ?2, ?3, ?4, ?5)",
        sql::params![data.0, data.1, data.2, data.3, data.4],
    )
    .unwrap();

    Ok(())
}

#[inline]
fn is_admin(member: impl Into<PartialMember>) -> bool {
    member.into().permissions.unwrap().administrator()
}

/**
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

struct CurrencyRow {
    terre: f64,
    air: f64,
    eau: f64,
    feu: f64,
    lumiere: f64,
}
impl Clone for CurrencyRow {
    fn clone(&self) -> Self {
        Self {
            terre: self.terre,
            air: self.air,
            eau: self.eau,
            feu: self.feu,
            lumiere: self.lumiere,
        }
    }
}
async fn get_data() -> Result<Vec<CurrencyRow>, sql::Error> {
    let conn = init_db()?;
    let mut stmt =
        conn.prepare("SELECT terre, air, eau, feu, lumiere FROM currency_data order by id ASC")?;
    let data = stmt
        .query_map([], |row| {
            Ok(CurrencyRow {
                terre: row.get(0)?,
                air: row.get(1)?,
                eau: row.get(2)?,
                feu: row.get(3)?,
                lumiere: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(data)
}

fn write_file<P, S>(path: P, contents: S) -> Result<(), io::Error>
where
    P: AsRef<std::path::Path>,
    S: Into<String>,
{
    let mut contents = contents.into();
    //Remove extra brackets that appear out of nowhere when converting json to string.
    if &contents[contents.len() - 2..] == "}}" {
        contents.pop();
    }
    fs::write(path, contents)?;

    Ok(())
}

/**
Calculate new value for currency by passing its current value and a chaos value.
*/
async fn calculate_rate(value: f64, chaos: f64) -> f64 {
    //The rate is the increase in value per day, in percentage.
    //It appears that the actual increase is half of the input value.
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
    let random = to_big_rationnal(rand::thread_rng().gen_range(0.0..=1.0)) * (&x - &y) + &y;

    //This is just a one step exponential function with our random number as a parameter.
    (to_big_rationnal(value) * (to_big_rationnal(1.0) + (random)))
        .to_f64()
        .unwrap()
}

#[inline]
/**
Convert float 64 to BigRationnal
*/
fn to_big_rationnal(x: f64) -> Ratio<BigInt> {
    BigRational::from_f64(x).unwrap()
}

struct CurrencyColumn {
    terre: Vec<f64>,
    air: Vec<f64>,
    eau: Vec<f64>,
    feu: Vec<f64>,
    lumiere: Vec<f64>,
}
impl Default for CurrencyColumn {
    fn default() -> Self {
        Self {
            terre: Vec::new(),
            air: Vec::new(),
            eau: Vec::new(),
            feu: Vec::new(),
            lumiere: Vec::new(),
        }
    }
}
fn get_currency_columns(data: &Vec<CurrencyRow>) -> CurrencyColumn {
    let mut cols = CurrencyColumn::default();

    for row in data {
        cols.terre.push(row.terre);
        cols.air.push(row.air);
        cols.eau.push(row.eau);
        cols.feu.push(row.feu);
        cols.lumiere.push(row.lumiere);
    }

    cols
}

async fn draw_graph() -> Result<(), sql::Error> {
    //Scoping here allows the graph to be dropped and the svg file created before we call render_svg.
    {
        let mut data: Vec<CurrencyRow> = get_data().await?;

        let root_drawing_area =
            SVGBackend::new("./images/graph.svg", (1440, 1080)).into_drawing_area();

        root_drawing_area.fill(&WHITE).unwrap();

        let vec_size = data.len() as f64;

        if vec_size > 28.0 {
            data = data[(vec_size - 28.0) as usize..].to_vec()
        }

        let max = data
            .iter()
            .map(|row| {
                row.terre
                    .max(row.air / 200.0)
                    .max(row.eau / 10.0)
                    .max(row.feu / 50.0)
                    .max(row.lumiere)
            })
            .fold(f64::MIN, f64::max);

        let min = data
            .iter()
            .map(|row| {
                row.terre
                    .min(row.air / 200.0)
                    .min(row.eau / 10.0)
                    .min(row.feu / 50.0)
                    .min(row.lumiere)
            })
            .fold(f64::MAX, f64::min);

        //Find where to start the graph on x axis
        let start;
        if vec_size < 28.0 {
            start = 0.0
        } else {
            start = vec_size - 28.0
        }

        let data_columns = get_currency_columns(&data);

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
                (data_columns.terre.iter()).map(|x| {
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
                (data_columns.air.iter()).map(|x| {
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
                (data_columns.eau.iter()).map(|x| {
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
                (data_columns.feu.iter()).map(|x| {
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
                (data_columns.lumiere.iter()).map(|x| {
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
    Ok(())
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

    init_db().unwrap();

    //Starting discord client
    let mut client = Client::builder(token, GatewayIntents::from(GatewayIntents::all()))
        .event_handler(Handler)
        .await
        .expect("Error creating client.");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
