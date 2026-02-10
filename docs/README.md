# Keywords Plugin

A plugin implementation for the [ConVerZen](https://converzen.de) **MCP server**.

Supply information by keyword search. 

## Concept
Instead of preemptive or agentic RAG searches with high latency, this plugin allows users to 
supply a keyword to morsel (information morsel) database in the form of a yaml file that associates a set of 
keywords with a **morsel** of information for the chat persona.

### Strategy

The keyword search uses a [trigram search](https://en.wikipedia.org/wiki/Trigram_search) to enable fuzzy searches
and generates a score for each keyword rather than an exact match.

### Analytics

The plugin optionally logs unmatched keywords to a failed keywords output file in json format.
This allows you to monitor the keywords that fail to produce valid results and improve your database.

### Logging
The plugin logs to stderr using env-logger. Logging is controlled through the ```RUST_LOG``` env
variable that can be set for the MCP instance in the *ConverZen* admin interface.

### Function Description
The plugin function description for `keywords_to_morsel` gives the LLM information about what this function can be 
used for. 
It has been changed from a static text to a dynamic configuration parameter 
`function_description` in the plugin configuration. 

It should reflect the purpose of this database. The plugin was originally 
developed for *Zeno* - *ConverZen*'s Expert Chat, which uses the following function description:  

```
"Use this tool to retrieve verified, high-priority information about specific product \
topics including pricing, security, technical stack, and feature shortcuts. This tool is \
faster and more accurate than a general knowledge base search for direct user inquiries. \
Input should be 1-2 core keywords (e.g., 'pricing', 'encryption', 'gdpr')."
```

### Future Extension

- Instead of using a local file, allow fetching the database from a remote server via
  HTTP GET Request.
- Add relevant / optional fields to the yaml database and internal format to make it future-proof
  for adaption in a broader context.


## Implementation Details

The plugin implements the initialisation asynchronously 
which at the current state (reading a file) is clearly overkill.
But it demonstrates how async initialisation can be done in 
a *ConverZen* plugin and future-proofs us for `HTML GET` 
remote databases. 

## Exported functions

The plugin declares one function: `keywords_to_morsel` with one parameter
`keywords`.
```rust
declare_tools! {
    tools: [
        Tool::builder("keywords_to_morsel", "Use this tool to retrieve verified, high-priority information about specific product topics including pricing, security, technical stack, and feature shortcuts. This tool is faster and more accurate than a general knowledge base search for direct user inquiries. Input should be 1-2 core keywords (e.g., 'pricing', 'encryption', 'gdpr').")
            .param_string("keywords", "Comma separated list of keywords", true)
            .handler(handle_get_morsel),
    ]
}
```

## Chatbot personality integration

Since we are building this in Rust, we care about speed and precision. 
You can give your chat persona a system prompt that says:

> "Before searching the general knowledge base, always check the
> `keywords_to_morsel` tool for direct shortcuts to save the
> user time."

This prioritizes your curated "morsels" over the potentially "noisy" RAG results.

## The Database

Sample database file:  
```yaml
- id: security_overview
  keywords: [ security, encryption, safe, protected, protocol ]
  link: /security
  content: |
    We take a security-first approach. All chat data is encrypted at rest using AES-256 and in transit via TLS 1.3. Our 
    Rust backend is memory-safe by design, eliminating common vulnerabilities like buffer overflows.

- id: gdpr_compliance
  keywords: [ gdpr, privacy, data, europe, compliance, dpa ]
  link: /privacy
  content: |
    Zeno is fully GDPR compliant. We offer data residency options and a self-service dashboard for Data Processing 
    Agreements (DPA). Users can request data deletion or exports at any time through the admin site.

- id: data_retention
  keywords: [ retention, storage, history, delete, logs ]
  link: /docs/retention
  content: |
    By default, chat history is stored for 30 days to power RAG features, but this is fully configurable in your 
    admin settings. You can set custom retention policies or trigger immediate purging via our API.
```


## Configuration

The following configuration items are supported: 
- **function_description:** *Required*. Function description for 'keywords_to_morsel' tool.
- **database_path:** *Required*. Path to a database/yaml file. Must be relative to the chat_server base 
directory.  
- **failed_keywords_path:** Path to a file that will contain information about failed 
keyword searches, if specified.  
- **morsel_n_best:** Maximum number of candidates to retrieve in fuzzy card name search, 
defaults to 1.
- **morsel_min_score:** Minimum score for a candidate in fuzzy card name search to 
make it to the result list.
- **update_interval_secs:**: Database update interval in seconds. Defaults to 3600 (1 hour)

**Sample configuration:** 
```json
{
  "function_description" : "Use this tool to retrieve verified, high-priority information about specific product topics including pricing, security, technical stack, and feature shortcuts.",
  "database_path": "./data/database.yaml",
  "failed_keywords_path": "./data/failed_keywords.log",
  "morsel_n_best": 2,
  "update_interval_secs" : 120
}
```
