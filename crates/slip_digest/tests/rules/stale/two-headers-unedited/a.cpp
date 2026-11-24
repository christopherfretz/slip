namespace legacy { int f(int); }
namespace legacy { int h; }
int u() { return legacy::f(1) + legacy::h; }
