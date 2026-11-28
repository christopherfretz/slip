namespace acme { int x(double a) { return (int)a; } }
namespace legacy { int x(int b) { return b; } }
int u() { return acme::x(1); }
