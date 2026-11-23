namespace legacy { int a; }
inline namespace acme { int f() { return legacy::a; } }
