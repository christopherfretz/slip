#ifdef B
namespace acme { int g(int); }
using namespace acme;
#else
namespace legacy { int g(int); }
using namespace legacy;
#endif
