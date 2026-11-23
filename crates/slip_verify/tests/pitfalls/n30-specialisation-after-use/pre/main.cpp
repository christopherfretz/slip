#include "dep.h"

namespace legacy {
struct Mine {};

template <> struct traits<Mine> { static const int v = 1; };

int use() { return traits<Mine>::v; }
}

int user() { return legacy::use(); }
