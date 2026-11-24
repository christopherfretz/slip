#include "dep.h"
namespace legacy { namespace v1 { int g(int); } }
int u() { return legacy::v1::g(1); }
