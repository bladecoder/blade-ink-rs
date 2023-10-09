#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>
#include <binkc.h>


void finish(int rc, struct binkc_story *story, char *err_msg) {
    binkc_cstring_free(err_msg);
    binkc_story_free(story);
    exit(rc);
}

void check_ret(int ret, struct binkc_story *story, char *err_msg) {
    if (ret != BINKC_OK) {
        printf("Error: %s\n", err_msg);
        finish(EXIT_FAILURE, story, err_msg);
    }
}

int main(void) {
    uint32_t ret = BINKC_OK;
    struct binkc_story *story = NULL;
    char *err_msg = NULL;
    char *line = NULL;
    char *json_string = "{\"inkVersion\":21,\"root\":[[\"^Line.\",\"\\n\",[\"done\",{\"#n\":\"g-0\"}],null],\"done\",null],\"listDefs\":{}}";

    ret = binkc_story_new(&story, json_string, &err_msg);

    check_ret(ret, story, err_msg);

    bool can_continue;
    ret = binkc_story_can_continue(story, &can_continue);
    check_ret(ret, story, err_msg);

    while (can_continue) {
        ret = binkc_story_cont(story, &line, &err_msg);
        check_ret(ret, story, err_msg);
        puts(line);
        binkc_cstring_free(line);
        
        ret = binkc_story_can_continue(story, &can_continue);
        check_ret(ret, story, err_msg);
    }

    printf("Ok.\n");

    finish(EXIT_SUCCESS, story, err_msg);
}