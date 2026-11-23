#define TWICE(e) ((e) + (e))
#define SHOW(e) ((e), #e)
int g(int);
int h(int);
int y = TWICE(g(1));
const char* z = (SHOW(g(2)), "");
