#include "dep.h"

namespace legacy {
int g(Old) { return 1; }
}

int user() { return legacy::g(legacy::Old()); }
