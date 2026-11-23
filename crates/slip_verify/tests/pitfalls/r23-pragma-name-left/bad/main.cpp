int g();
#pragma weak f
int g() { return 1; }

int caller() { return g(); }
