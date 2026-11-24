#include "dep.h"

namespace acme {
int g(legacy::Old) { return 1; }
}

int user() { return acme::g(legacy::Old()); }
