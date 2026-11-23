#include "dep.h"

namespace legacy {
struct Mine {};
int touch(Dep) { return 1; }
}

int user() { legacy::Mine m; legacy::Dep d; return touch(d); }
