#include "dep.h"

namespace legacy {
int g(int x) { return x; }
int h(int x) { return x; }
}

int user() { return legacy::g(1) + legacy::h(2); }
