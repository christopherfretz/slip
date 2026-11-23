#include "dep.h"

namespace legacy {
struct Mine {};
extern "C" void c_hook(int);
}

void user() { legacy::Mine m; legacy::c_hook(1); }
