#include "cost_unbounded_concurrency.hpp"

std::vector<std::future<std::string>> load_every_page(const PageClient& client, const std::vector<std::string>& ids) {
  std::vector<std::future<std::string>> pending;
  for (const std::string& id : ids) {
    pending.push_back(std::async(std::launch::async, [&client, id] { return client.get(id); }));
  }
  return pending;
}

std::vector<std::future<std::string>> load_few_pages(const PageClient& client, const std::vector<std::string>& ids) {
  std::vector<std::future<std::string>> pending;
  const std::size_t limit = 4;
  for (std::size_t at = 0; at < ids.size() && at < limit; ++at) {
    const std::string id = ids[at];
    pending.push_back(std::async(std::launch::async, [&client, id] { return client.get(id); }));
  }
  return pending;
}
