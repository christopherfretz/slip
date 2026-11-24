#include "dep.h"

namespace acme {
namespace detail {
int g(int x) { return x; }
}
}

int user() { return acme::detail::g(1); }
