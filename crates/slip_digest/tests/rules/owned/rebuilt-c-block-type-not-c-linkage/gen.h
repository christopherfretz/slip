#ifdef POST
extern "C" { namespace acme { struct Mine { int v; }; } }
#else
extern "C" { namespace legacy { struct Mine { int v; }; } }
#endif
