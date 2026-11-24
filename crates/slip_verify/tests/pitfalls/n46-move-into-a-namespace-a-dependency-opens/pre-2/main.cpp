#include "dep.h"

namespace legacy {
namespace detail {
int g(int x) { return x; }
}
}

int user() { return legacy::detail::g(1); }
