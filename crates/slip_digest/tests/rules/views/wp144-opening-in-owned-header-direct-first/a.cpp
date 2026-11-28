#include "own_open.h"
#include "ro_inc.h"
namespace legacy { int g(int); }
using namespace legacy;
int u() { return g(1); }
