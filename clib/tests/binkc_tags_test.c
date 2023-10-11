#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>
#include <string.h>
#include <binkc.h>


void finish(int rc, struct binkc_story *story, char *err_msg) {
    binkc_cstring_free(err_msg);
    binkc_story_free(story);
    exit(rc);
}

void check_ret(int ret, struct binkc_story *story, char *err_msg) {
    if (ret != BINKC_OK) {
        if(err_msg != NULL)
            perror(err_msg);
        
        finish(EXIT_FAILURE, story, err_msg);
    }
}

char* read_json_file(const char* filename) {
    FILE* file = fopen(filename, "r");
    if (!file) {
        perror("Failed to open file");
        return NULL;
    }

    fseek(file, 0, SEEK_END);
    long fileSize = ftell(file);
    fseek(file, 0, SEEK_SET);

    char* jsonString = (char*)malloc(fileSize + 1);
    if (!jsonString) {
        perror("Memory allocation failed");
        fclose(file);
        return NULL;
    }

    size_t bytesRead = fread(jsonString, 1, fileSize, file);
    if ((long)bytesRead != fileSize) {
        perror("Failed to read file");
        free(jsonString);
        fclose(file);
        return NULL;
    }

    jsonString[fileSize] = '\0';

    fclose(file);

    return jsonString;
}


int main(void) {
    uint32_t ret = BINKC_OK;
    struct binkc_story *story = NULL;
    struct binkc_tags *tags = NULL;
    char *err_msg = NULL;
    char *line = NULL;

    char *json_string = read_json_file("../inkfiles/tags/tagsDynamicContent.ink.json");
    if(json_string == NULL)
        exit(EXIT_FAILURE);

    ret = binkc_story_new(&story, json_string, &err_msg);
    check_ret(ret, story, err_msg);
    free(json_string);

    ret = binkc_story_cont(story, &line, &err_msg);
    check_ret(ret, story, err_msg);
    puts(line);

    if (strcmp(line, "tag\n") != 0) {
        puts("expected line");
        finish(EXIT_FAILURE, NULL, NULL);
    }

    binkc_cstring_free(line);

    // Obtain and print tags
    size_t len = 0;
    ret = binkc_story_get_current_tags(story, &tags, &len);
    check_ret(ret, story, NULL);

    if (len != 1) {
        printf("expected len==1, actual=%lu", len);
        finish(EXIT_FAILURE, story, NULL);
    }

    char *tag = NULL;
    ret = binkc_tags_get(tags, 0, &tag);
    if (ret != BINKC_OK) {
        puts("error getting tag 0");
        finish(EXIT_FAILURE, NULL, NULL);
    }

    printf("TAG: %s\n", tag);

    if (strcmp(tag, "pic8red.jpg") != 0 )
        finish(EXIT_FAILURE, NULL, NULL);

    binkc_cstring_free(tag);

    puts("Story ended ok.\n");

    finish(EXIT_SUCCESS, story, err_msg);
}