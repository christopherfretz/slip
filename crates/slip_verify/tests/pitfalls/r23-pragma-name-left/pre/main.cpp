int f();
#pragma weak f
int f() { return 1; }

int caller() { return f(); }
