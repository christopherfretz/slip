#include "new_dep.h"
namespace legacy { namespace detail { int g(int); } }
using namespace legacy::detail;
int u() { return g(1); }
