#include "base.h"
#include "dep.h"

namespace acme {
struct Mine {};
}

void user() { acme::Mine m; legacy::Dep d; }
