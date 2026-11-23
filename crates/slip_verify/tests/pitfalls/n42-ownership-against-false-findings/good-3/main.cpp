#include "dep.h"

namespace acme {
struct Mine {};
} namespace legacy {
int touch(Dep) { return 1; }
}

int user() { acme::Mine m; legacy::Dep d; return touch(d); }
