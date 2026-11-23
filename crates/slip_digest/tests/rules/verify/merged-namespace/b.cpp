namespace legacy {
int a;
}
namespace acme {
int f() { return legacy::a; }
}
