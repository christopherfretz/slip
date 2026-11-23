#include "dep.h"

namespace legacy {
struct Mine {};
int hook(Mine) { return 1; }
}

int caller() { return legacy::probe(legacy::Mine{}); }
