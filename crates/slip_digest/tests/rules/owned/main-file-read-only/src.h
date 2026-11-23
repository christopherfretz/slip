#ifdef POST
namespace acme { struct Mine { int v; }; int f(Mine*); }
#else
namespace legacy { struct Mine { int v; }; int f(Mine*); }
#endif
