#ifdef B
namespace legacy {}
namespace acme { int g(int); }
using namespace acme;
using namespace legacy;
#else
namespace legacy { int g(int); }
using namespace legacy;
#endif
