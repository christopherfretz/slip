#include "dep.h"
#include "acme.h"

namespace acme {
struct Mine {};
} namespace legacy {
extern "C" int c_fn(int);
}

int user() { acme::Mine m; return legacy::c_fn(1); }
