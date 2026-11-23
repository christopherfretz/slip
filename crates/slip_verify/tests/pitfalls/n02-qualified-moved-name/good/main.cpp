#include "dep.h"

namespace acme {
namespace x {
struct T {};
}
}

void user() { acme::x::T t; }
