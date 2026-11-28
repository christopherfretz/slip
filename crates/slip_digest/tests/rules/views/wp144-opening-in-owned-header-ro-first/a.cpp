#include "ro_inc.h"
#include "own_open.h"
namespace legacy { int g(int); }
using namespace legacy;
int u() { return g(1); }
