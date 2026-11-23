#include "dep.h"

namespace legacy {
int run(int n) { return n + 1; }
}

int caller() { return LEGACY_CALL(run, 1); }
