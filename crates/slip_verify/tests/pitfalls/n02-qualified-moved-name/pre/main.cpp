#include "dep.h"

namespace legacy {
namespace x {
struct T {};
}
}

void user() { legacy::x::T t; }
