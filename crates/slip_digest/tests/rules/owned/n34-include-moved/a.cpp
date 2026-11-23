#include "dep.h"

namespace legacy {
#include "inner.h"

struct Mine {};
}

void user() { legacy::Mine m; legacy::Inner i; }
