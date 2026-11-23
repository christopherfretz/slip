int g();
#pragma weak g
int g() { return 1; }

int caller() { return g(); }
