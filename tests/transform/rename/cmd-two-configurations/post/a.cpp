#ifdef HAVE_F
namespace a { int g(int x) { return x; } }
int u() { return a::g(1); }
#endif
int w() { return 0; }
