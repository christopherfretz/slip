#include "dep.h"

namespace acme {
namespace x {
struct T {};
}
}

void user() { legacy::x::T t; }
