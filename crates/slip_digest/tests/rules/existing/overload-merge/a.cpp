namespace acme { int x(int a) { return a; } }
namespace legacy { int x(double b) { return (int)b; } }
int u() { return acme::x(1) + legacy::x(2.0); }
