#include "dep.h"
const int limit = 4;
namespace legacy {
int take(int n = limit) { return n; }
}

int caller() { return legacy::take(); }
