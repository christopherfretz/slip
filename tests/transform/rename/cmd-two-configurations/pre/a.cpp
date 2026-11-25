#ifdef HAVE_F
namespace a { int f(int x) { return x; } }
int u() { return a::f(1); }
#endif
int w() { return 0; }
