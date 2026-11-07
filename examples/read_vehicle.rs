use renault_api::{Auth, Config, RenaultApi};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let token = std::env::var("RENAULT_SESSION_TOKEN")?;
    let api = RenaultApi::new(Config::new("fr_FR")?)?;
    api.authenticate(Auth::SessionToken(token)).await?;

    for account in api.accounts().await? {
        for vehicle in api.vehicles(&account.id).await? {
            let cockpit = api.cockpit(&account.id, &vehicle.vin).await?;
            println!("{}: {:?} km", vehicle.vin, cockpit.total_mileage_km);
        }
    }
    Ok(())
}
