#include "dep.h"
using namespace acme;
namespace legacy { int g(int); }
int u() { Dep d; return legacy::g(1); }
