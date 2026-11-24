#include "dep.h"
namespace legacy { inline namespace v1 { int g(int); } }
int u() { return legacy::g(1); }
