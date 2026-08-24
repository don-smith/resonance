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

  type ViewerOptions = Pick<
    EditorOptions,
    "el" | "initialValue" | "usageStatistics"
  > & { viewer: true };

  export default class Editor {
    constructor(options: EditorOptions);
    static factory(options: ViewerOptions): Editor;
    destroy(): void;
    getMarkdown(): string;
  }
}
