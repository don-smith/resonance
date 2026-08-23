declare module "@toast-ui/editor" {
  type ToolbarItem = string | { name: string };

  type EditorOptions = {
    el: HTMLElement;
    height?: string;
    initialEditType?: "markdown" | "wysiwyg";
    initialValue?: string;
    hideModeSwitch?: boolean;
    usageStatistics?: boolean;
    toolbarItems?: Array<ToolbarItem | ToolbarItem[]>;
  };

  export default class Editor {
    constructor(options: EditorOptions);
    destroy(): void;
    getMarkdown(): string;
  }
}
