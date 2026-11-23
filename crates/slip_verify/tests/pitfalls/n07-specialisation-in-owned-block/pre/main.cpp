#include "dep.h"

namespace legacy {
struct Mine {};
template <> struct traits<Mine> { static const int value = 1; };
}

int caller() { return legacy::traits<legacy::Mine>::value; }
