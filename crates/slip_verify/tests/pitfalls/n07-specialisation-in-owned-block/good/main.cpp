#include "dep.h"

namespace acme {
struct Mine {};
} namespace legacy {
template <> struct traits<acme::Mine> { static const int value = 1; };
}

int caller() { return legacy::traits<acme::Mine>::value; }
