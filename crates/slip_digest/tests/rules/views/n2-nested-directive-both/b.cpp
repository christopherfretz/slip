#include "new_dep.h"
namespace acme { namespace detail { int g(int); } }
using namespace acme::detail;
int u() { return g(1); }
