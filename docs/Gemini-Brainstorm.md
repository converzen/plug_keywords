This is a classic "cold start" problem in RAG. Relying on users to write perfectly semantic queries is a recipe for
missed hits, especially when they use shorthand like "pricing?" instead of "What are the various subscription tiers and
costs associated with this service?"

Using an **MCP (Model Context Protocol)** tool to bridge the gap between vague user intent and specific documentation
morsels is a very smart move. It essentially acts as a "Router" or a "Curated Shortcut" that bypasses the noise of a
full vector search.

### The "Keyword-to-Morsel" Strategy

In MCP implementations, this is often referred to as **Contextual Routing**. Instead of letting the LLM wander through a
massive vector space, you give it a "map" (your trigram-backed keyword tool).

Here is a breakdown of useful keyword-to-content pairings and how to optimize that MCP implementation.

---

### 1. Essential Keyword-to-Content Pairings

Think of these as "Fast-Pass" tickets for Zeno. You want to capture intent-heavy words that are often buried in
long-form RAG docs.

| Keyword Category    | Target Keywords                                           | Morsel Content Focus                                         |
|---------------------|-----------------------------------------------------------|--------------------------------------------------------------|
| **Connectivity**    | `API`, `Integration`, `Webhooks`, `Zapier`, `SDK`         | Quick-start auth guide & base URL.                           |
| **Trust/Legal**     | `Security`, `GDPR`, `Privacy`, `Compliance`, `Encryption` | Link to Data Processing Agreement + brief security summary.  |
| **Commercial**      | `Price`, `Cost`, `Free`, `Trial`, `Enterprise`, `Billing` | Current tier breakdown and "Contact Sales" link.             |
| **Technical Stack** | `Rust`, `Latency`, `Streaming`, `Wasm`, `Architecture`    | High-level system diagram description and performance specs. |
| **Human Touch**     | `Human`, `Support`, `Help`, `Email`, `Talk to a person`   | Support hours and the actual support email/ticket link.      |

---

### 2. Common Patterns in MCP Implementations

Since you are using a Rust backend and an MCP plugin structure, here are a few tips to make Zeno more effective:

* **The "Intent Switchboard":** Don't just return text. Have your MCP tool return a **Type**. For example, if a user
  hits the `Pricing` keyword, the tool returns `{ "type": "cta", "content": "...", "link": "/pricing" }`. This allows
  Zeno to not just *talk* about it, but explicitly say, "You can find the full breakdown on our pricing page
  here [Link]."
* **Synonym Expansion (The "Alias" Array):** Trigrams are great for typos, but they won't link "Money" to "Pricing." Map
  your morsels to arrays of aliases:
* *Morsel:* Deployment
* *Aliases:* `["install", "setup", "hosting", "docker", "on-prem"]`


* **Negative Keywords:** To avoid "hallucination-by-keyword," ensure your tool can return a "No specific match found"
  signal. This tells Zeno to fall back to the standard RAG search rather than forcing a keyword match.

### 3. Improving the "Where can I..." Problem

For your webpage directory MCP, you can enhance the retrieval by including **Action-Oriented Keywords**.
Users don't just look for "Settings"; they ask "How do I change my password?"

* **Key-Value Pair:** `["password", "email", "account", "delete"]` -> **Content:** "Account Management" -> **URL:**
  `/dashboard/settings`

---

### Pro-Tip for Zeno's Personality

Since you’re building this in Rust, you likely care about speed and precision. You can give Zeno a system prompt that
says:

> "Before searching the general knowledge base, always check the `product_info` tool for direct shortcuts to save the
> user time."

This prioritizes your curated "morsels" over the potentially "noisy" RAG results.

**Would you like me to help you draft the specific "morsel" text for any of those technical categories, like the Rust
backend or the chat widget integration?**