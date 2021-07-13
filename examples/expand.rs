fn main() {
    let question = discourse::Question::expand("overwrite")
        .message("Conflict on `file.rs`")
        .choices(vec![
            ('y', "Overwrite"),
            ('a', "Overwrite this one and all next"),
            ('d', "Show diff"),
        ])
        .default_separator()
        .choice('x', "Abort")
        .build();

    println!("{:#?}", discourse::prompt_one(question));
}
