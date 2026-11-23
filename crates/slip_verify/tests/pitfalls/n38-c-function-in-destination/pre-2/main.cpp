#include "dep.h"
#include "acme.h"

namespace legacy {
struct Mine {};
extern "C" int c_fn(int);
}

int user() { legacy::Mine m; return legacy::c_fn(1); }
