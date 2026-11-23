#include "other.h"
namespace legacy { struct Mine {}; namespace al = other; using other::thing; using namespace other; }
int use() { legacy::thing t; return legacy::al::value; }
