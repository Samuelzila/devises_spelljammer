const CONFIG_PATH: &str = "./config";

use chrono::Local;
use plotters::{
    coord::types::{RangedCoordf64, RangedCoordusize},
    prelude::*,
    style::full_palette::{BROWN, LIGHTBLUE, YELLOW_800},
};
use rand::rng;
use rand_distr::{Distribution, Normal};
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
use std::{fs, io, path::Path, time::Duration};

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
        let annual_rate = 0.075;
        reference_terre = calculate_rate(reference_terre, annual_rate, 0.01).await;
        reference_air = calculate_rate(reference_air, annual_rate, 0.50).await;
        reference_eau = calculate_rate(reference_eau, annual_rate, 0.03).await;
        reference_feu = calculate_rate(reference_feu, annual_rate, 0.05).await;
        reference_lum = calculate_rate(reference_lum, annual_rate, 0.01).await;

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

fn is_admin(member: impl Into<PartialMember>) -> bool {
    member.into().permissions.unwrap().administrator()
}

/**
Sends a permission denied error to the channel.
*/
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
async fn calculate_rate(value: f64, target_annual_rate: f64, chaos: f64) -> f64 {
    let days_in_year: f64 = 365.2422;

    let mu = ((1.0 + target_annual_rate).powf(1.0 / days_in_year)) - (1.0);

    let sigma = chaos / days_in_year.sqrt();

    let normal = Normal::new(mu, sigma).unwrap();

    let daily_rate = normal.sample(&mut rng());

    //This is just a one step exponential function with our random number as a parameter.
    value * (1.0 + daily_rate)
}

struct CurrencyColumns {
    terre: Vec<f64>,
    air: Vec<f64>,
    eau: Vec<f64>,
    feu: Vec<f64>,
    lumiere: Vec<f64>,
}
impl Default for CurrencyColumns {
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
fn get_currency_columns(data: &Vec<CurrencyRow>) -> CurrencyColumns {
    let mut cols = CurrencyColumns::default();

    for row in data {
        cols.terre.push(row.terre);
        cols.air.push(row.air);
        cols.eau.push(row.eau);
        cols.feu.push(row.feu);
        cols.lumiere.push(row.lumiere);
    }

    cols
}

fn add_currency_to_graph(
    chart: &mut ChartContext<'_, SVGBackend, Cartesian2d<RangedCoordusize, RangedCoordf64>>,
    series: Vec<f64>,
    label: &str,
    start_index: usize,
    color: RGBColor,
    multiplier: f64,
) {
    chart
        .draw_series(LineSeries::new(
            series
                .iter()
                .enumerate()
                .map(|(x, y)| (x + start_index, *y * multiplier)),
            &color,
        ))
        .unwrap()
        .label(label)
        .legend(move |(x, y)| Circle::new((x, y), 5, color.filled()));
}

async fn draw_graph() -> Result<(), sql::Error> {
    //Scoping here allows the graph to be dropped and the svg file created before we call render_svg.
    {
        let mut data: Vec<CurrencyRow> = get_data().await?;

        let root_drawing_area =
            SVGBackend::new("./images/graph.svg", (1440, 1080)).into_drawing_area();

        root_drawing_area.fill(&WHITE).unwrap();

        let vec_size = data.len();

        if vec_size > 28 {
            data = data[(vec_size - 28)..].to_vec()
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
        let start: usize;
        if vec_size < 28 {
            start = 0
        } else {
            start = vec_size - 28
        }

        let data_columns = get_currency_columns(&data);

        //Create the chart
        let mut chart = ChartBuilder::on(&root_drawing_area)
            .set_left_and_bottom_label_area_size(60)
            .margin_right(60)
            .margin_top(60)
            .build_cartesian_2d(start..(vec_size - 1), min..max)
            .unwrap();

        chart
            .configure_mesh()
            .label_style(("sans-serif", 20))
            .y_label_formatter(&|y| format!("{:.4}", y))
            //Set the amount of grid lines as the smallest number between 28 and the amount of integers in the domain.
            .x_labels(28.min(vec_size))
            .draw()
            .unwrap();

        //Draw all the curves
        add_currency_to_graph(&mut chart, data_columns.terre, "Terre", start, BROWN, 1.);
        add_currency_to_graph(
            &mut chart,
            data_columns.air,
            "Air",
            start,
            LIGHTBLUE,
            1. / 200.,
        );
        add_currency_to_graph(&mut chart, data_columns.eau, "Eau", start, BLUE, 1. / 10.);
        add_currency_to_graph(&mut chart, data_columns.feu, "Feu", start, RED, 1. / 50.);
        add_currency_to_graph(
            &mut chart,
            data_columns.lumiere,
            "Lumière/Ténèbre",
            start,
            YELLOW_800,
            1.,
        );

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
