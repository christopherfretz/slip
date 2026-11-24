namespace legacy { int f(int); }
namespace acme { int h; }
int u() { return legacy::f(1) + acme::h; }
