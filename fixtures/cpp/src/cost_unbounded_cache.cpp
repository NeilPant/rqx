#include "cost_unbounded_cache.hpp"

#include <unordered_map>

static std::unordered_map<std::string, int> page_sizes;
static std::unordered_map<std::string, int> recent_sizes;

int remember_page(const std::string& key, int size) {
  page_sizes[key] = size;
  return size;
}

int remember_recent_page(const std::string& key, int size) {
  recent_sizes[key] = size;
  if (recent_sizes.size() > 100) {
    recent_sizes.clear();
  }
  return size;
}
