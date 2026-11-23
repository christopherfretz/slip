namespace legacy { int a; }
namespace [[deprecated]] acme { int f() { return legacy::a; } }
