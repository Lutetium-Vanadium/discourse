include!("templates/pizza.rs");

// It has to be called `tokio_dep` in this example due to implementation reasons. When
// used outside this crate, just `tokio` will work.
#[tokio_dep::main]
async fn main() -> inquisition::Result<()> {
    // There is no special async prompt, PromptModule itself can run both synchronously
    // and asynchronously
    let mut module = inquisition::PromptModule::new(pizza_questions());

    // you can also prompt a single question, and get a mutable reference to its answer
    if module.prompt_async().await?.unwrap().as_bool().unwrap() {
        println!("Delivery is guaranteed to be under 40 minutes");
    }

    println!("{:#?}", module.prompt_all_async().await?);

    Ok(())
}