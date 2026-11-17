import asyncio


async def load_every_page(client, urls: list[str]):
    return await asyncio.gather(*[client.get(url) for url in urls])


async def load_few_pages(client, urls: list[str]):
    limit = asyncio.Semaphore(4)

    async def bounded(url: str):
        async with limit:
            return await client.get(url)

    return await asyncio.gather(*[bounded(url) for url in urls])


async def load_two_pages(client, first: str, second: str):
    return await asyncio.gather(client.get(first), client.get(second))
