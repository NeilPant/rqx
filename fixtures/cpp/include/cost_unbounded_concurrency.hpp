#pragma once

#include <future>
#include <string>
#include <vector>

struct PageClient {
  std::string get(const std::string& id) const { return id; }
};

std::vector<std::future<std::string>> load_every_page(const PageClient& client, const std::vector<std::string>& ids);
std::vector<std::future<std::string>> load_few_pages(const PageClient& client, const std::vector<std::string>& ids);
