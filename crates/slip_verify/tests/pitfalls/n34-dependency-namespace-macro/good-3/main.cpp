#include "dep.h"

namespace legacy {
#include "inner.h"
} namespace acme {
struct Mine {};
}

void user() { acme::Mine m; legacy::Inner i; }
