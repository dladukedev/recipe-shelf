# Recipe Shelf

A self-hosted recipe browser for TOML recipe collections. Search and filter recipes, manage accounts, and export recipes as text, Markdown, or PDF with customizable templates.

## Set up with Docker Compose

Install Docker with the Compose plugin on your server.

Save the following as `compose.yaml`. Replace `./recipes` with the path to your recipe directory on the server.

```yaml
services:
  recipe-shelf:
    image: ghcr.io/dladukedev/recipe-shelf:latest
    ports:
      - "3000:3000"
    volumes:
      - recipe-shelf-data:/data
      - ./recipes:/recipes:ro
    restart: unless-stopped

volumes:
  recipe-shelf-data:
```

Run this from the directory containing `compose.yaml`:

```sh
docker compose up -d
```

Open the site in your browser and create the initial administrator account. Under **Settings → Folders**, add `/recipes`, then sync to index your collection. Use `/recipes` here even if the directory has a different name on your server; this is its path inside the container.

Recipe files are mounted read-only. Syncing or removing a folder in Recipe Shelf never changes your source files.

## Health checks

`GET /health` returns HTTP `200` with `{"status":"ok"}` when the web server is responding. It requires no authentication and works before initial setup. This is a liveness check; it does not check the database or recipe folders.

```sh
curl --fail http://localhost:3000/health
```

## Backups

Accounts, settings, export templates, and the recipe index are stored in the `recipe-shelf-data` Docker volume. Back up this volume and your original recipe directory.

## TOML Format

Save each recipe as a `.toml` file in your recipe directory, for example `toll-house-cookies.toml`. Only `title` is required. Ingredients are a multiline string; instructions, notes, categories, and cuisines are arrays of strings.
Times are expressed in minutes.

This example is adapted from [Nestlé Toll House’s Original Chocolate Chip Cookies](https://www.verybestbaking.com/toll-house/recipes/chocolate-chip-cookies/), with instructions paraphrased for this example.

```toml
title = "Toll House Chocolate Chip Cookies"
description = "Chocolate chip cookies with optional chopped nuts. Adapted from Nestlé Toll House: https://www.verybestbaking.com/toll-house/recipes/chocolate-chip-cookies/"
author = "Nestlé Toll House"
yield = "5 dozen cookies"
categories = ["Dessert", "Cookies"]
cuisines = ["American"]
prep_time = 15
cook_time = 9

ingredients = """
2 1/4 cups all-purpose flour
1 teaspoon baking soda
1 teaspoon salt
1 cup softened butter (2 sticks)
3/4 cup white sugar
3/4 cup packed brown sugar
1 teaspoon vanilla extract
2 large eggs
2 cups Nestlé Toll House semi-sweet chocolate chips (12 ounces)
1 cup chopped nuts (optional)
"""

instructions = [
  "Set the oven to 375°F. In one bowl, mix the flour with the baking soda and salt.",
  "Cream the butter with both sugars and vanilla. Beat in each egg separately, then gradually mix in the dry ingredients. Fold in the chocolate chips and nuts.",
  "Portion rounded tablespoons of dough onto baking sheets without greasing them. Bake each batch for 9–11 minutes, until golden.",
  "Leave the cookies on the sheets for 2 minutes, then transfer to racks to finish cooling.",
]

notes = [
  "If leaving out the nuts, add another 1–2 tablespoons of flour.",
  "Cook time is per batch; allow extra time for baking the full yield.",
]
```
