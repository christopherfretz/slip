#include "dep.h"

namespace legacy {
extern "C" void c_hook(int);
}

void user() { legacy::c_hook(1); }
