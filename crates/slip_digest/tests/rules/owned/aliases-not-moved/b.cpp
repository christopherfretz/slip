#include "other.h"
namespace legacy { namespace al = other; using other::thing; using namespace other; }
namespace acme { struct Mine {}; }
int use() { legacy::thing t; return legacy::al::value; }
