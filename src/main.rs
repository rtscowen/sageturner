use argh::FromArgs;

mod docker;

#[derive(Debug, FromArgs, PartialEq)]
#[argh(description="SimpleSage gets your models to AWS SageMaker in one step")]
struct SimpleSageCLI {
    #[argh(subcommand)]
    nested: SimpleSageSubCommands
}

#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand)]
enum SimpleSageSubCommands {
    Deploy(Deploy),
    Setup(Setup)
}

#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand, name="deploy", description="Deploy serialised models directly to Sagemaker endpoint")]
struct Deploy {
    #[argh(option, short='w', description="wire config path")]
    wire_file: String, 
    
    #[argh(option, short='d', description="dockerfile path")]
    dockerfile: String,

    #[argh(option, short='e', description="endpoint type")]
    endpoint_type: String,
}


#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand, name="setup", description="Bootstrap sagemaker with necessary setup - incurs no cost")]
struct Setup {}

#[::tokio::main]
async fn main() {
    let cmd : SimpleSageCLI = argh::from_env();

    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let client = aws_sdk_sagemaker::Client::new(&config);


    match cmd.nested {
        SimpleSageSubCommands::Deploy(deploy) => process_deploy(&client, &deploy).await,
        SimpleSageSubCommands::Setup(setup) => process_setup(&client, &setup).await,
    }
}

async fn process_deploy(client: &aws_sdk_sagemaker::Client, deploy_params: &Deploy) {

    // parse the wire YAML 
    // build the image programatically 
    // push the image to ECR 
    // 

    // // Create a model, specifying this container
    // client.create_model().containers(input)

    // // Reference the container as a production variant in the endpoint config (serverless)
    // let production_variants: Vec<String> = vec![]


    // client.create_endpoint_config()
    //     .set_endpoint_config_name(Some("endpoint_1".to_string()))
    //     .set_production_variants(input);
    
    // // Create the endpoint 
    // client.create_endpoint();
}

async fn process_setup(client: &aws_sdk_sagemaker::Client, setup_params: &Setup) {
    todo!()
}