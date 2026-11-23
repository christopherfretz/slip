#include "dep.h"

namespace legacy {
int take(int n = limit) { return n; }
}

int caller() { return legacy::take(); }
