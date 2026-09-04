#pragma once
#include <string>
#include <vector>

struct Row {
    std::string status;
    int amount;
    int retries;
};

int total_of(const std::vector<Row>& rows);
int record_value(int value);
int notify_value(int value);
int first(const std::vector<Row>& rows);
int second(const std::vector<Row>& entries);

class Reconciler {
private:
    int ledger = 0;
public:
    int reconcile_batch(const std::vector<Row>& batch, int tolerance, bool dry_run, int retries, bool verbose);
};
