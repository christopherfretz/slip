#ifdef POST
namespace acme { struct Mine {
#include "members.inc"
}; }
#else
namespace legacy { struct Mine {
#include "members.inc"
}; }
#endif
